using Microsoft.Win32;

namespace AutoFix.Services;

/// <summary>Starts AutoFix in the tray when the user signs in.</summary>
internal static class StartupRegistration
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "AutoFix";

    public static void Apply(bool enabled)
    {
        try
        {
            using var key = Registry.CurrentUser.CreateSubKey(RunKey);
            if (enabled)
            {
                string exe = Environment.ProcessPath ?? throw new InvalidOperationException("Unknown executable path.");
                key.SetValue(ValueName, $"\"{exe}\" --background");
            }
            else if (key.GetValue(ValueName) is not null)
            {
                key.DeleteValue(ValueName);
            }
        }
        catch (Exception ex) when (ex is UnauthorizedAccessException or System.Security.SecurityException or IOException
            or InvalidOperationException)
        {
            Log.Error("Could not update startup registration", ex);
        }
    }
}
