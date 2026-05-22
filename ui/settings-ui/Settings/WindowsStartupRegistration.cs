using System.Diagnostics.CodeAnalysis;
using System.IO;
using Microsoft.Win32;

namespace AutoFix.SettingsUi.Settings;

public sealed class WindowsStartupRegistration : IStartupRegistration
{
    private const string AppName = "AutoFix";
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private readonly Func<string> commandFactory;

    public WindowsStartupRegistration() : this(BackgroundCommand)
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

    internal static string BackgroundCommand() => $"{Quote(BackgroundExecutablePath())} --background";

    private static string BackgroundExecutablePath()
    {
        var baseDirectory = AppContext.BaseDirectory;
        var direct = Path.Combine(baseDirectory, "background-engine.exe");
        if (File.Exists(direct))
        {
            return direct;
        }

        var sibling = Path.GetFullPath(Path.Combine(baseDirectory, "..", "background-engine", "background-engine.exe"));
        if (File.Exists(sibling))
        {
            return sibling;
        }

        return direct;
    }

    [SuppressMessage("Design", "CA1307:Specify StringComparison for clarity", Justification = "Character replacement overload.")]
    private static string Quote(string value) => $"\"{value.Replace("\"", "\\\"")}\"";
}
