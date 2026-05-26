using System.Diagnostics;
using System.IO;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class ShutdownHelperLauncher : IShutdownHelperLauncher
{
    public bool TryLaunchShutdownAll()
    {
        var path = AutofixHostPath.Find();
        if (path is null)
        {
            return false;
        }

        Process.Start(CreateStartInfo(path));
        return true;
    }

    internal static ProcessStartInfo CreateStartInfo(string path)
    {
        return new ProcessStartInfo
        {
            FileName = path,
            Arguments = Program.ShutdownAllArgument,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
    }
}
