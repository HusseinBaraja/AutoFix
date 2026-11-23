using System.Text.Json;
using System.Text.Json.Serialization;

namespace AutoFix.Core.Settings;

/// <summary>Reads and writes <c>settings.json</c>. API keys are never stored here.</summary>
public sealed class SettingsStore(string path)
{
    internal static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        ReadCommentHandling = JsonCommentHandling.Skip,
        AllowTrailingCommas = true,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.CamelCase) },
    };

    public string Path { get; } = path;

    /// <summary>Loads settings. A missing or damaged file gives defaults; a damaged file is kept as <c>.bad</c>.</summary>
    public AppSettings Load()
    {
        if (!File.Exists(Path)) return new AppSettings();
        try
        {
            var settings = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(Path), JsonOptions) ?? new AppSettings();
            return Normalize(settings);
        }
        catch (Exception ex) when (ex is JsonException or IOException or NotSupportedException)
        {
            try
            {
                File.Copy(Path, Path + ".bad", overwrite: true);
            }
            catch (IOException)
            {
                // Keeping a copy is best effort.
            }
            return new AppSettings();
        }
    }

    public void Save(AppSettings settings)
    {
        Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
        string temp = Path + ".tmp";
        File.WriteAllText(temp, JsonSerializer.Serialize(settings, JsonOptions));
        File.Move(temp, Path, overwrite: true);
    }

    private static AppSettings Normalize(AppSettings settings)
    {
        settings.PauseMs = Math.Clamp(settings.PauseMs, 300, 10_000);
        settings.Providers ??= new();
        settings.BlockedApps ??= [];
        settings.Dictionary ??= [];
        if (string.IsNullOrWhiteSpace(settings.Hotkey) || KeyChord.TryParse(settings.Hotkey) is null)
            settings.Hotkey = new AppSettings().Hotkey;
        return settings;
    }
}
