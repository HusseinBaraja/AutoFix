using System.Diagnostics.CodeAnalysis;
using System.IO;
using Microsoft.Win32;

namespace AutoFix.SettingsUi.Settings;

public sealed class WindowsStartupRegistration : IStartupRegistration
{
    private const string AppName = "AutoFix";
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private readonly Func<string> commandFactory;

    public WindowsStartupRegistration() : this(ShellCommand)
    {
    }

    internal WindowsStartupRegistration(Func<string> commandFactory)
    {
        this.commandFactory = commandFactory;
    }

    public void Apply(bool startWithWindows)
    {
        using var runKey = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true)
            ?? throw new InvalidOperationException("Could not open current-user startup registry key.");

        if (startWithWindows)
        {
            runKey.SetValue(AppName, commandFactory(), RegistryValueKind.String);
            return;
        }

        runKey.DeleteValue(AppName, throwOnMissingValue: false);
    }

    internal static string ShellCommand() => Quote(ShellExecutablePath());

    private static string ShellExecutablePath()
    {
        var baseDirectory = AppContext.BaseDirectory;
        var direct = Path.Combine(baseDirectory, "Autofix.exe");
        if (File.Exists(direct))
        {
            return direct;
        }

        return direct;
    }

    [SuppressMessage("Design", "CA1307:Specify StringComparison for clarity", Justification = "Character replacement overload.")]
    private static string Quote(string value) => $"\"{value.Replace("\"", "\\\"")}\"";
}
