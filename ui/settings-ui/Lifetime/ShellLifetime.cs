using System.ComponentModel;
using System.Diagnostics;
using System.Threading;
using System.Windows;
using WpfApplication = System.Windows.Application;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class ShellLifetime : IDisposable
{
    private readonly WpfApplication application;
    private readonly EngineSupervisor engineSupervisor;
    private readonly ProcessJob processJob;
    private readonly SingleInstance singleInstance;
    private ShellTray? tray;
    private Window? window;
    private int shutdownInProgress;

    public ShellLifetime(
        WpfApplication application,
        EngineSupervisor engineSupervisor,
        ProcessJob processJob,
        SingleInstance singleInstance)
    {
        this.application = application;
        this.engineSupervisor = engineSupervisor;
        this.processJob = processJob;
        this.singleInstance = singleInstance;
        this.engineSupervisor.EngineExitClassified += EngineExitClassified;
    }

    public static ShellLifetime Create(WpfApplication application)
    {
        SingleInstance? single = null;
        single = SingleInstance.Create(() =>
        {
            application.Dispatcher.Invoke(() => application.MainWindow?.ShowAndFocus());
        });
        return new ShellLifetime(
            application,
            new EngineSupervisor(new EngineProcessLauncher()),
            new ProcessJob(),
            single);
    }

    public bool OwnsInstance => singleInstance.OwnsInstance;

    public async Task SignalExistingAsync() => await singleInstance.SignalExistingAsync();

    public void Start(Window shellWindow)
    {
        window = shellWindow;
        AppWindowIdentity.Apply(shellWindow);
        application.MainWindow = shellWindow;
        application.ShutdownMode = ShutdownMode.OnExplicitShutdown;
        shellWindow.Closing += WindowClosing;
        try
        {
            tray = new ShellTray(ShowShell, () => Shutdown(ShutdownReason.UserExit));
            singleInstance.StartListening();
            engineSupervisor.Start();
            shellWindow.Show();
        }
        catch (Exception error)
        {
            Debug.WriteLine(error);
            RollBackStartup(shellWindow);
            Shutdown(ShutdownReason.UserExit);
        }
    }

    private void RollBackStartup(Window shellWindow)
    {
        engineSupervisor.StopIntentionally(ShutdownReason.UserExit);
        tray?.Dispose();
        tray = null;
        singleInstance.Dispose();
        shellWindow.Closing -= WindowClosing;
        if (shellWindow.IsVisible)
        {
            shellWindow.Close();
        }
    }

    public void Shutdown(ShutdownReason reason)
    {
        if (Interlocked.Exchange(ref shutdownInProgress, 1) != 0)
        {
            return;
        }

        engineSupervisor.StopIntentionally(reason);
        tray?.Dispose();
        application.Shutdown();
    }

    private void WindowClosing(object? sender, CancelEventArgs e)
    {
        if (Volatile.Read(ref shutdownInProgress) != 0)
        {
            return;
        }

        e.Cancel = true;
        window?.Hide();
    }

    private void ShowShell() => application.Dispatcher.Invoke(() => window?.ShowAndFocus());

    private void EngineExitClassified(object? sender, EngineExitClassification classification)
    {
        if (classification is EngineExitClassification.Intentional or EngineExitClassification.Restarted)
        {
            return;
        }

        application.Dispatcher.Invoke(() => Shutdown(ShutdownReason.ShellClosing));
    }

    public void Dispose()
    {
        engineSupervisor.EngineExitClassified -= EngineExitClassified;
        tray?.Dispose();
        engineSupervisor.Dispose();
        processJob.Dispose();
        singleInstance.Dispose();
    }
}

public static class WindowActivationExtensions
{
    public static void ShowAndFocus(this Window window)
    {
        if (!window.IsVisible)
        {
            window.Show();
        }

        if (window.WindowState == WindowState.Minimized)
        {
            window.WindowState = WindowState.Normal;
        }

        window.Activate();
        window.Topmost = true;
        window.Topmost = false;
        window.Focus();
    }
}
