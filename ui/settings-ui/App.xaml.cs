using System.Windows;
using System.Windows.Threading;
using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi;

public partial class App : Application
{
    private readonly BackgroundIpcClient ipcClient = new();
    private AppLifetimeShutdown? shutdown;
    private BackgroundAvailabilityMonitor? backgroundMonitor;
    private DispatcherTimer? monitorTimer;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        shutdown = new AppLifetimeShutdown(ipcClient, new ShutdownHelperLauncher());
        backgroundMonitor = new BackgroundAvailabilityMonitor(ipcClient);
        monitorTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(1),
        };
        monitorTimer.Tick += MonitorBackground;
        monitorTimer.Start();
    }

    protected override async void OnExit(ExitEventArgs e)
    {
        monitorTimer?.Stop();
        if (shutdown is not null)
        {
            await shutdown.RequestShutdownAllAsync();
        }

        base.OnExit(e);
    }

    protected override async void OnSessionEnding(SessionEndingCancelEventArgs e)
    {
        if (shutdown is not null)
        {
            await shutdown.RequestShutdownAllAsync();
        }

        base.OnSessionEnding(e);
    }

    private async void MonitorBackground(object? sender, EventArgs e)
    {
        if (backgroundMonitor is not null && await backgroundMonitor.ShouldCloseSettingsAsync())
        {
            Shutdown();
        }
    }
}
