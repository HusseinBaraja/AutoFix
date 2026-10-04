using System.Windows;

namespace AutoFix.SettingsUi.Settings;

public interface ISettingsConsent
{
    bool ConfirmFullTextDebug();
    bool ConfirmClearLogs();
}

public sealed class SettingsConsent : ISettingsConsent
{
    public bool ConfirmFullTextDebug() => Confirm("Full-text debug logging can store private text you type, including personal or confidential content, on this computer. Anyone with access to these logs may read it. Secure fields remain blocked. Enable only while troubleshooting, then disable it and clear logs.\n\nEnable full-text debug logging?", "Privacy warning");

    public bool ConfirmClearLogs() => Confirm("Delete all correction metadata and debug events, including any saved full-text diagnostics? This cannot be undone. Dictionary entries and app rules are kept.", "Clear logs");

    private static bool Confirm(string message, string title) => MessageBox.Show(message, title,
        MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No) == MessageBoxResult.Yes;
}
