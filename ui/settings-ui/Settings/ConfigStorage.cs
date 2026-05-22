using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Tomlyn;

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

    public AppConfig LoadOrCreate()
    {
        if (!File.Exists(ConfigPath))
        {
            var config = AppConfig.Default();
            Save(config);
            LastLoadCreatedConfig = true;
            return config;
        }

        LastLoadCreatedConfig = false;
        return Load(ConfigPath);
    }

    public AppConfig Load(string path)
    {
        var text = File.ReadAllText(path);
        var config = TomlSerializer.Deserialize<AppConfig>(text, TomlOptions)
            ?? throw new InvalidDataException("Config file was empty.");
        ConfigValidator.Validate(config);
        return config;
    }

    public void Save(AppConfig config)
    {
        ConfigValidator.Validate(config);
        var directory = Path.GetDirectoryName(ConfigPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        File.WriteAllText(ConfigPath, ToToml(config), Encoding.UTF8);
    }

    public void Import(string sourcePath)
    {
        var imported = Load(sourcePath);
        Save(imported);
    }

    public void Export(string destinationPath, AppConfig config)
    {
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
        # Store API keys in the OS secret store or environment, not in this TOML file.
        # Shortcut format uses key names joined by '+', for example Ctrl+Alt+Space.
        # Correction streaming stays disabled because corrections need bounded latency.

        """;
}
