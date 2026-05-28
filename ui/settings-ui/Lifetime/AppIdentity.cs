using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Lifetime;

internal static class AppIdentity
{
    internal const string AppUserModelId = "Zerone.Autofix";

    internal static void SetCurrentProcess()
    {
        var result = SetCurrentProcessExplicitAppUserModelID(AppUserModelId);
        if (result < 0)
        {
            Marshal.ThrowExceptionForHR(result);
        }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern int SetCurrentProcessExplicitAppUserModelID(string appId);
}
