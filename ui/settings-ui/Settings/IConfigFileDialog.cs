using Microsoft.Win32;

namespace AutoFix.SettingsUi.Settings;

public interface IConfigFileDialog
{
    string? PickImportPath();
    string? PickExportPath();
}

public sealed class ConfigFileDialog : IConfigFileDialog
{
    private const string ImportFilter = "AutoFix bundle (*.zip)|*.zip|TOML settings (*.toml)|*.toml";

    public string? PickImportPath()
    {
        var dialog = new OpenFileDialog
        {
            Filter = ImportFilter,
            Title = "Preview AutoFix import",
            CheckFileExists = true,
        };

        return dialog.ShowDialog() == true ? dialog.FileName : null;
    }

    public string? PickExportPath()
    {
        var dialog = new SaveFileDialog
        {
            Filter = "AutoFix bundle (*.zip)|*.zip",
            Title = "Export AutoFix settings and rules",
            FileName = "AutoFix-settings.zip",
            DefaultExt = ".zip",
            AddExtension = true,
            OverwritePrompt = true,
        };

        return dialog.ShowDialog() == true ? dialog.FileName : null;
    }
}
