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
    private readonly object engineLock = new();
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

    public bool IsRunning
    {
        get
        {
            lock (engineLock)
            {
                return engine is { HasExited: false };
            }
        }
    }

    public void Start()
    {
        lock (engineLock)
        {
            if (engine is { HasExited: false })
            {
                return;
            }

            StartNewEngine();
        }
    }

    public void StopIntentionally(ShutdownReason reason)
    {
        IEngineProcess? running = null;
        lock (engineLock)
        {
            restartPolicy.Clear();
            intentionalStop.Mark(reason);
            if (engine is { HasExited: false } current)
            {
                running = current;
            }
            else
            {
                intentionalStop.Clear();
            }
        }

        running?.Kill();
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
        EngineExitClassification? classification = null;

        lock (engineLock)
        {
            if (sender is not IEngineProcess exitedEngine || !ReferenceEquals(exitedEngine, engine))
            {
                return;
            }

            if (disposed)
            {
                return;
            }

            if (intentionalStop.HasCurrentStop)
            {
                intentionalStop.Clear();
                classification = EngineExitClassification.Intentional;
            }
            else if (exitedEngine.ExitCode == 1)
            {
                classification = EngineExitClassification.ExternalKill;
            }
            else if (!restartPolicy.TryRecordAttempt())
            {
                classification = EngineExitClassification.RestartExhausted;
            }
            else
            {
                try
                {
                    StartNewEngine();
                    classification = EngineExitClassification.Restarted;
                }
                catch (Exception error) when (error is IOException or InvalidOperationException or System.ComponentModel.Win32Exception)
                {
                    classification = EngineExitClassification.StartFailed;
                }
            }

        }

        if (classification is { } exitClassification)
        {
            EngineExitClassified?.Invoke(this, exitClassification);
        }
    }

    public void Dispose()
    {
        IEngineProcess? current;
        lock (engineLock)
        {
            disposed = true;
            current = engine;
        }

        current?.Dispose();
    }
}
