using System.Diagnostics;
using System.IO;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class ShutdownHelperLauncher : IShutdownHelperLauncher
{
    public bool TryLaunchShutdownAll()
    {
        var path = FindBackgroundEnginePath();
        if (path is null)
        {
            return false;
        }

        Process.Start(new ProcessStartInfo
        {
            FileName = path,
            Arguments = "--shutdown-all",
            UseShellExecute = false,
            CreateNoWindow = true,
        });
        return true;
    }

    private static string? FindBackgroundEnginePath()
    {
        var baseDirectory = AppContext.BaseDirectory;
        var candidates = new[]
        {
            Path.Combine(baseDirectory, "AF-BG-Engine.exe"),
            Path.GetFullPath(Path.Combine(baseDirectory, "..", "..", "..", "..", "..", "target", "debug", "AF-BG-Engine.exe")),
            Path.GetFullPath(Path.Combine(baseDirectory, "..", "..", "..", "..", "..", "target", "release", "AF-BG-Engine.exe")),
        };

        return candidates.FirstOrDefault(File.Exists);
    }
}
