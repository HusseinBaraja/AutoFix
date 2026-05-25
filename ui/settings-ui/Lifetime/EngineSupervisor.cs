using System.IO;

namespace AutoFix.SettingsUi.Lifetime;

public enum EngineExitClassification
{
    Intentional,
    ExternalKill,
    Restarted,
    RestartExhausted,
    StartFailed,
}

public sealed class EngineSupervisor : IDisposable
{
    private readonly IEngineProcessLauncher launcher;
    private readonly RestartPolicy restartPolicy;
    private readonly IntentionalEngineStop intentionalStop;
    private IEngineProcess? engine;
    private bool disposed;

    public EngineSupervisor(
        IEngineProcessLauncher launcher,
        RestartPolicy? restartPolicy = null,
        IntentionalEngineStop? intentionalStop = null)
    {
        this.launcher = launcher;
        this.restartPolicy = restartPolicy ?? new RestartPolicy();
        this.intentionalStop = intentionalStop ?? new IntentionalEngineStop();
    }

    public event EventHandler<EngineExitClassification>? EngineExitClassified;

    public bool IsRunning => engine is { HasExited: false };

    public void Start()
    {
        if (IsRunning)
        {
            return;
        }

        StartNewEngine();
    }

    public void StopIntentionally(ShutdownReason reason)
    {
        restartPolicy.Clear();
        intentionalStop.Mark(reason);
        if (engine is { HasExited: false } running)
        {
            running.Kill();
            return;
        }

        intentionalStop.Clear();
    }

    private void StartNewEngine()
    {
        intentionalStop.Clear();
        engine?.Dispose();
        engine = launcher.Start();
        engine.Exited += EngineExited;
    }

    private void EngineExited(object? sender, EventArgs e)
    {
        if (disposed)
        {
            return;
        }

        if (intentionalStop.HasCurrentStop)
        {
            intentionalStop.Clear();
            EngineExitClassified?.Invoke(this, EngineExitClassification.Intentional);
            return;
        }

        if (engine?.ExitCode == 1)
        {
            EngineExitClassified?.Invoke(this, EngineExitClassification.ExternalKill);
            return;
        }

        if (!restartPolicy.TryRecordAttempt())
        {
            EngineExitClassified?.Invoke(this, EngineExitClassification.RestartExhausted);
            return;
        }

        try
        {
            StartNewEngine();
            EngineExitClassified?.Invoke(this, EngineExitClassification.Restarted);
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            EngineExitClassified?.Invoke(this, EngineExitClassification.StartFailed);
        }
    }

    public void Dispose()
    {
        disposed = true;
        engine?.Dispose();
    }
}
