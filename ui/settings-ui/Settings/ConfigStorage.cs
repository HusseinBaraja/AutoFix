using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Tomlyn;
using AutoFix.SettingsUi.Settings.Transfer;

namespace AutoFix.SettingsUi.Settings;

public sealed class ConfigStorage
{
    private static readonly TomlSerializerOptions TomlOptions = new()
    {
        PreferredObjectCreationHandling = JsonObjectCreationHandling.Replace,
        DefaultIgnoreCondition = TomlIgnoreCondition.WhenWritingNull,
        WriteIndented = true,
    };

    public ConfigStorage() : this(DefaultConfigPath())
    {
    }

    public ConfigStorage(string configPath)
    {
        ConfigPath = configPath;
    }

    public string ConfigPath { get; }
    public bool LastLoadCreatedConfig { get; private set; }

    /// <summary>Loads validated settings or persists defaults when no settings file exists.</summary>
    public AppConfig LoadOrCreate()
        => LoadSnapshot().Config;

    /// <summary>Parses the same file bytes retained for stale-preview detection, creating defaults when missing.</summary>
    internal (AppConfig Config, byte[] Bytes) LoadSnapshot()
    {
        using var access = ConfigFileAccess.Acquire(ConfigPath);
        ImportRecovery.Recover(ConfigPath);
        return ReadSnapshot();
    }

    /// <summary>Reads settings while the caller already owns the settings-file lock and has completed recovery.</summary>
    internal (AppConfig Config, byte[] Bytes) ReadSnapshot()
    {
        if (!File.Exists(ConfigPath))
        {
            var config = AppConfig.Default();
            WriteConfig(config);
            LastLoadCreatedConfig = true;
        }
        else LastLoadCreatedConfig = false;

        var bytes = File.ReadAllBytes(ConfigPath);
        using var reader = new StreamReader(new MemoryStream(bytes), Encoding.UTF8, detectEncodingFromByteOrderMarks: true);
        return (Parse(reader.ReadToEnd()), bytes);
    }

    /// <summary>Loads TOML, normalizes legacy grammar categories and retry counts, and rejects invalid settings.</summary>
    public AppConfig Load(string path)
    {
        if (!Path.GetFullPath(path).Equals(Path.GetFullPath(ConfigPath), StringComparison.OrdinalIgnoreCase))
            return Parse(File.ReadAllText(path));
        using var access = ConfigFileAccess.Acquire(path);
        ImportRecovery.Recover(path);
        return Parse(File.ReadAllText(path));
    }

    internal static AppConfig Parse(string text)
    {
        AppConfig config;
        try
        {
            config = TomlSerializer.Deserialize<AppConfig>(text, TomlOptions)
                ?? throw new InvalidDataException("Config file was empty.");
        }
        catch (TomlException error)
        {
            throw new InvalidDataException($"Invalid settings TOML: {error.Message}", error);
        }
        config.Correction.EnabledGrammarCategories = config.Correction.EnabledGrammarCategories
            .Select(category => category == "punctuation" ? "spacing" : category)
            .Distinct()
            .ToList();
        if (config.Api.RetryCount > 1)
        {
            config.Api.RetryCount = 1;
        }
        ConfigValidator.Validate(config);
        return config;
    }

    public void Save(AppConfig config)
    {
        ConfigValidator.Validate(config);
        using var access = ConfigFileAccess.Acquire(ConfigPath);
        if (ImportRecovery.Recover(ConfigPath))
            throw new InvalidDataException("An interrupted import was recovered. Reload settings before saving edits.");
        WriteConfig(config);
    }

    private void WriteConfig(AppConfig config)
    {
        var directory = Path.GetDirectoryName(ConfigPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var temporary = ConfigPath + ".save.tmp";
        ConfigFileAccess.WriteDurable(temporary, Encoding.UTF8.GetBytes(ToToml(config)));
        try { ConfigFileAccess.ReplaceDurable(temporary, ConfigPath); }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public void Import(string sourcePath)
    {
        var imported = Load(sourcePath);
        Save(imported);
    }

    public void Export(string destinationPath, AppConfig config)
    {
        if (Path.GetFullPath(destinationPath).Equals(Path.GetFullPath(ConfigPath), StringComparison.OrdinalIgnoreCase))
        {
            Save(config);
            return;
        }
        ConfigValidator.Validate(config);
        var directory = Path.GetDirectoryName(destinationPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        File.WriteAllText(destinationPath, ToToml(config), Encoding.UTF8);
    }

    public static string ToToml(AppConfig config)
    {
        return GeneratedComments() + TomlSerializer.Serialize(config, TomlOptions);
    }

    private static string DefaultConfigPath()
    {
        var root = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        if (string.IsNullOrWhiteSpace(root))
        {
            root = Environment.CurrentDirectory;
        }

        return Path.Combine(root, "AutoFix", "settings.toml");
    }

    private static string GeneratedComments() =>
        """
        # AutoFix user configuration.
        # Store API keys in Windows Credential Manager, not in this TOML file.
        # Shortcut format uses key names joined by '+', for example Ctrl+Alt+Space.
        # Correction streaming stays disabled because corrections need bounded latency.
        # Pending queue size counts running and waiting corrections per session (1 to 16).
        # Full queue: skip_new, cancel_oldest, or merge_newest and wait for the next trigger.

        """;
}
