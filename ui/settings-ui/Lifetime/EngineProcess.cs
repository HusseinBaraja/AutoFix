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
        var path = AutofixHostPath.Find();
        if (path is null)
        {
            throw new FileNotFoundException("Autofix.exe was not found.");
        }

        var process = Process.Start(CreateStartInfo(path))
            ?? throw new InvalidOperationException("Autofix.exe --engine did not start.");

        process.EnableRaisingEvents = true;
        return new RealEngineProcess(process);
    }

    internal static ProcessStartInfo CreateStartInfo(string path) => new()
    {
        FileName = path,
        Arguments = Program.EngineArgument,
        UseShellExecute = false,
        CreateNoWindow = true,
        WorkingDirectory = Path.GetDirectoryName(path),
    };

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

public static class AutofixHostPath
{
    public static string? Find()
    {
        if (Environment.ProcessPath is { } currentProcess && IsAutofixHostPath(currentProcess))
        {
            return currentProcess;
        }

        var baseDirectory = AppContext.BaseDirectory;
        var candidates = new[]
        {
            Path.Combine(baseDirectory, "Autofix.exe"),
            Path.GetFullPath(Path.Combine(baseDirectory, "..", "..", "..", "..", "..", "ui", "settings-ui", "bin", "Debug", "net8.0-windows", "Autofix.exe")),
            Path.GetFullPath(Path.Combine(baseDirectory, "..", "..", "..", "..", "..", "ui", "settings-ui", "bin", "Release", "net8.0-windows", "Autofix.exe")),
        };

        return candidates.FirstOrDefault(File.Exists);
    }

    internal static bool IsAutofixHostPath(string path) =>
        File.Exists(path)
        && Path.GetFileName(path).Equals("Autofix.exe", StringComparison.OrdinalIgnoreCase);
}
