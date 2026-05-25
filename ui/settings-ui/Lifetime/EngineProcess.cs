using System.Diagnostics;
using System.IO;

namespace AutoFix.SettingsUi.Lifetime;

public interface IEngineProcess : IDisposable
{
    event EventHandler? Exited;
    int Id { get; }
    int? ExitCode { get; }
    bool HasExited { get; }
    void Kill();
}

public interface IEngineProcessLauncher
{
    IEngineProcess Start();
}

public sealed class EngineProcessLauncher : IEngineProcessLauncher
{
    public IEngineProcess Start()
    {
        var path = EnginePath.Find();
        if (path is null)
        {
            throw new FileNotFoundException("AF-BG-Engine.exe was not found.");
        }

        var process = Process.Start(new ProcessStartInfo
        {
            FileName = path,
            Arguments = "--background",
            UseShellExecute = false,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(path),
        }) ?? throw new InvalidOperationException("AF-BG-Engine.exe did not start.");

        process.EnableRaisingEvents = true;
        return new RealEngineProcess(process);
    }

    private sealed class RealEngineProcess : IEngineProcess
    {
        private readonly Process process;

        public RealEngineProcess(Process process)
        {
            this.process = process;
            this.process.Exited += (_, _) => Exited?.Invoke(this, EventArgs.Empty);
        }

        public event EventHandler? Exited;
        public int Id => process.Id;
        public int? ExitCode => process.HasExited ? process.ExitCode : null;
        public bool HasExited => process.HasExited;

        public void Kill()
        {
            if (!process.HasExited)
            {
                process.Kill();
            }
        }

        public void Dispose() => process.Dispose();
    }
}

public static class EnginePath
{
    public static string? Find()
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
