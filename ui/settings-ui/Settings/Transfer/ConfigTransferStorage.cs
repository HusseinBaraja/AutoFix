using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Models;

namespace AutoFix.SettingsUi.Settings.Transfer;

/// <summary>Transfers an explicit allowlist of persistent product data, never a database backup.</summary>
public sealed partial class ConfigTransferStorage(ConfigStorage configStorage, AppRuleStorage appRuleStorage)
{
    private const int MaxFileBytes = 8 * 1024 * 1024;
    private const int MaxRows = 10000;
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
        WriteIndented = true,
    };
    private static readonly string[] RequiredFiles = ["manifest.json", "settings.toml", "app-rules.json", "dictionary.json", "language-overrides.json"];

    public void Export(string destination, AppConfig settings, bool includeLearnedRules)
    {
        if (Path.GetFullPath(destination).Equals(Path.GetFullPath(configStorage.ConfigPath), StringComparison.OrdinalIgnoreCase)
            || Path.GetFullPath(destination).Equals(Path.GetFullPath(appRuleStorage.DatabasePath), StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Choose a bundle destination different from AutoFix's live settings and database.");
        ConfigValidator.Validate(settings);
        EnsureProductTables();
        using var connection = OpenDatabase();
        using var transaction = connection.BeginTransaction(deferred: true);
        var data = ReadData(connection, transaction, includeLearnedRules);
        transaction.Commit();
        var config = ConfigStorage.Parse(ConfigStorage.ToToml(settings));
        var languages = data.LanguageOverrides.ToDictionary(row => row.Process, row => row.Language, StringComparer.OrdinalIgnoreCase);
        foreach (var entry in config.Correction.AppLanguageOverrides)
        {
            var parts = entry.Split('=', 2);
            languages[parts[0].Trim().ToLowerInvariant()] = parts[1].Trim();
        }
        data = data with { LanguageOverrides = languages.OrderBy(pair => pair.Key, StringComparer.OrdinalIgnoreCase)
            .Select(pair => new LanguageOverride(pair.Key, pair.Value)).ToArray() };
        config.Correction.AppLanguageOverrides = data.LanguageOverrides.Select(row => $"{row.Process}={row.Language}").ToList();
        var bundle = Validate(new(config, data));

        var directory = Path.GetDirectoryName(Path.GetFullPath(destination))!;
        Directory.CreateDirectory(directory);
        var temporary = Path.Combine(directory, $".autofix-export-{Guid.NewGuid():N}.tmp");
        try
        {
            using (var archive = ZipFile.Open(temporary, ZipArchiveMode.Create))
            {
                WriteJson(archive, "manifest.json", new BundleManifest(1, includeLearnedRules));
                WriteText(archive, "settings.toml", ConfigStorage.ToToml(bundle.Settings));
                WriteJson(archive, "app-rules.json", bundle.Data!.AppRules);
                WriteJson(archive, "dictionary.json", bundle.Data.Dictionary);
                WriteJson(archive, "language-overrides.json", bundle.Data.LanguageOverrides);
                if (includeLearnedRules) WriteJson(archive, "learned-rules.json", bundle.Data.LearnedRules);
            }
            File.Move(temporary, destination, overwrite: true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public ConfigImportPreview Preview(string source)
    {
        var bundle = ReadBundle(source); // Validate everything before reading or changing local storage.
        EnsureProductTables();
        using var connection = OpenDatabase();
        using var transaction = connection.BeginTransaction(deferred: true);
        var currentData = ReadData(connection, transaction, bundle.Data?.LearnedRules is not null);
        var currentConfig = configStorage.LoadOrCreate();
        var fingerprint = Fingerprint(currentData);
        transaction.Commit();
        return new(Path.GetFileName(source), bundle, DescribeChanges(currentConfig, currentData, bundle), fingerprint);
    }

    private static ConfigBundle ReadBundle(string source)
    {
        if (new FileInfo(source).Length > MaxFileBytes * 7L) throw new InvalidDataException("Import file is too large.");
        if (Path.GetExtension(source).Equals(".toml", StringComparison.OrdinalIgnoreCase))
        {
            if (new FileInfo(source).Length > MaxFileBytes) throw new InvalidDataException("Settings file is too large.");
            return Validate(new(ConfigStorage.Parse(File.ReadAllText(source)), null));
        }
        using var archive = ZipFile.OpenRead(source);
        var entries = new Dictionary<string, ZipArchiveEntry>(StringComparer.Ordinal);
        foreach (var entry in archive.Entries)
        {
            if (!RequiredFiles.Contains(entry.FullName) && entry.FullName != "learned-rules.json")
                throw new InvalidDataException("Bundle contains an unsupported file. Only settings and product rules may be imported.");
            if (!entries.TryAdd(entry.FullName, entry) || entry.Length > MaxFileBytes)
                throw new InvalidDataException("Bundle contains a duplicate or oversized file.");
        }
        if (RequiredFiles.Any(name => !entries.ContainsKey(name))) throw new InvalidDataException("Bundle is missing a required file.");
        var manifest = ReadJson<BundleManifest>(entries["manifest.json"]);
        if (manifest.FormatVersion != 1 || manifest.LearnedRulesIncluded != entries.ContainsKey("learned-rules.json"))
            throw new InvalidDataException("Unsupported bundle version or inconsistent learned-rule manifest.");
        return Validate(new(ConfigStorage.Parse(ReadText(entries["settings.toml"])), new(
            ReadJson<List<AppRuleDto>>(entries["app-rules.json"]), ReadJson<List<DictionaryEntry>>(entries["dictionary.json"]),
            manifest.LearnedRulesIncluded ? ReadJson<List<LearnedRule>>(entries["learned-rules.json"]) : null,
            ReadJson<List<LanguageOverride>>(entries["language-overrides.json"]))));
    }

    private static ConfigBundle Validate(ConfigBundle bundle)
    {
        ConfigValidator.Validate(bundle.Settings);
        if (bundle.Data is not { } data) return bundle;
        var apps = new List<AppRuleDto>();
        foreach (var dto in Limit(data.AppRules))
        {
            if (dto is null || dto.ProcessName is null || dto.ListBehavior is null)
                throw new InvalidDataException("App rules must include a process name and list behavior.");
            var item = AppRuleStorage.FromDto(dto); AppRuleStorage.Validate(item);
            item.ProcessName = item.ProcessName.Trim().ToLowerInvariant();
            apps.Add(AppRuleStorage.ToDto(item));
        }
        Unique(apps.Select(row => JsonSerializer.Serialize(new[] { row.ProcessName, row.WindowTitlePattern ?? "" })), "app rule scopes");
        var dictionary = new List<DictionaryEntry>();
        foreach (var entry in Limit(data.Dictionary))
        {
            if (entry is null || entry.Word is null || entry.Language is null) throw new InvalidDataException("Dictionary entries need a word and language.");
            DictionaryStorage.Validate(new() { Word = entry.Word, Language = entry.Language, App = entry.App ?? "", Source = "dictionary" });
            dictionary.Add(entry with { Language = entry.Language.Trim(), App = NormalizeApp(entry.App) });
        }
        Unique(dictionary.Select(row => JsonSerializer.Serialize(new[] { row.Word, row.Language.ToLowerInvariant(), row.App })), "dictionary scopes");
        List<LearnedRule>? learned = data.LearnedRules is null ? null : [];
        foreach (var entry in Limit(data.LearnedRules ?? []))
        {
            if (entry is null || entry.Original is null || entry.RuleType is null
                || entry.RuleType is not ("pair" or "dictionary" or "never_change_x_to_y"))
                throw new InvalidDataException("Learned rule has an unsupported type.");
            DictionaryStorage.Validate(new() { Word = entry.Original, Language = entry.Language ?? "und", App = entry.App ?? "",
                Source = entry.RuleType == "dictionary" ? "dictionary" : "pair", Replacement = entry.Replacement ?? "" });
            learned!.Add(entry with { App = NormalizeApp(entry.App), Language = entry.Language?.Trim() });
        }
        if (learned is not null) Unique(learned.Select(LearnedIdentity), "learned rule scopes");
        var languages = new List<LanguageOverride>();
        foreach (var entry in Limit(data.LanguageOverrides))
        {
            if (entry is null || entry.Process is null || entry.Language is null || !ConfigValidator.ValidLanguageTag(entry.Language))
                throw new InvalidDataException("Language override needs a process name and valid language tag.");
            AppRuleStorage.Validate(new() { ProcessName = entry.Process });
            languages.Add(entry with { Process = entry.Process.Trim().ToLowerInvariant() });
        }
        Unique(languages.Select(row => row.Process), "language override scopes");
        var configured = bundle.Settings.Correction.AppLanguageOverrides.Select(row => row.Split('=', 2))
            .Select(parts => $"{parts[0].Trim().ToLowerInvariant()}={parts[1].Trim().ToLowerInvariant()}").Order().ToArray();
        var stored = languages.Select(row => $"{row.Process}={row.Language.ToLowerInvariant()}").Order().ToArray();
        if (!configured.SequenceEqual(stored)) throw new InvalidDataException("Language overrides disagree with settings.toml.");
        return bundle with { Data = new(apps, dictionary, learned, languages) };
    }

    private static IEnumerable<T> Limit<T>(IReadOnlyList<T> rows)
    {
        if (rows.Count > MaxRows) throw new InvalidDataException("Bundle has too many product rules.");
        return rows;
    }
    private static void Unique(IEnumerable<string> keys, string name)
    {
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var key in keys) if (!seen.Add(key)) throw new InvalidDataException($"Bundle contains duplicate {name}.");
    }
    private static string? NormalizeApp(string? app) => string.IsNullOrWhiteSpace(app) ? null : app.Trim().ToLowerInvariant();
    private static string LearnedIdentity(LearnedRule row) => JsonSerializer.Serialize(new[] { row.Original, row.Replacement, row.RuleType,
        row.Language?.ToLowerInvariant() ?? "und", NormalizeApp(row.App) });
    private static T ReadJson<T>(ZipArchiveEntry entry) => JsonSerializer.Deserialize<T>(ReadText(entry), JsonOptions)
        ?? throw new InvalidDataException("Bundle file must contain valid product data.");
    private static string ReadText(ZipArchiveEntry entry)
    {
        using var stream = entry.Open();
        using var buffer = new MemoryStream();
        var bytes = new byte[8192];
        int count;
        while ((count = stream.Read(bytes)) > 0)
        {
            if (buffer.Length + count > MaxFileBytes) throw new InvalidDataException("Bundle file is too large.");
            buffer.Write(bytes, 0, count);
        }
        return new UTF8Encoding(false, true).GetString(buffer.ToArray());
    }
    private static void WriteJson<T>(ZipArchive archive, string name, T value) => WriteText(archive, name, JsonSerializer.Serialize(value, JsonOptions));
    private static void WriteText(ZipArchive archive, string name, string text)
    {
        if (Encoding.UTF8.GetByteCount(text) > MaxFileBytes) throw new InvalidDataException("Export data is too large.");
        using var writer = new StreamWriter(archive.CreateEntry(name).Open(), new UTF8Encoding(false));
        writer.Write(text);
    }
    private string Fingerprint(TransferData data)
    {
        var state = File.ReadAllText(configStorage.ConfigPath) + JsonSerializer.Serialize(data, JsonOptions);
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(state)));
    }
}
