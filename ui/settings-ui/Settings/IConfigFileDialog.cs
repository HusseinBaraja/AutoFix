using Microsoft.Win32;

namespace AutoFix.SettingsUi.Settings;

public interface IConfigFileDialog
{
    string? PickImportPath();
    string? PickExportPath();
}

public sealed class ConfigFileDialog : IConfigFileDialog
{
    private const string Filter = "TOML config (*.toml)|*.toml|All files (*.*)|*.*";

    public string? PickImportPath()
    {
        var dialog = new OpenFileDialog
        {
            Filter = Filter,
            Title = "Import AutoFix config",
            CheckFileExists = true,
        };

        return dialog.ShowDialog() == true ? dialog.FileName : null;
    }

    public string? PickExportPath()
    {
        var dialog = new SaveFileDialog
        {
            Filter = Filter,
            Title = "Export AutoFix config",
            FileName = "settings.toml",
            OverwritePrompt = true,
        };

        return dialog.ShowDialog() == true ? dialog.FileName : null;
    }
}
