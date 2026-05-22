using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

public sealed class TempConfigFixture : IDisposable
{
    private readonly string root;

    private TempConfigFixture(string root)
    {
        this.root = root;
        Path = System.IO.Path.Combine(root, "settings.toml");
        Storage = new ConfigStorage(Path);
    }

    public string Root => root;
    public string Path { get; }
    public ConfigStorage Storage { get; }

    public static TempConfigFixture Create()
    {
        var root = System.IO.Path.Combine(System.IO.Path.GetTempPath(), $"autofix-settings-{Guid.NewGuid():N}");
        Directory.CreateDirectory(root);
        return new TempConfigFixture(root);
    }

    public void Dispose()
    {
        Directory.Delete(root, true);
    }
}
