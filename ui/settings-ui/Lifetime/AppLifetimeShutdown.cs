using AutoFix.SettingsUi.Ipc;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class AppLifetimeShutdown
{
    private readonly IBackgroundIpcClient ipcClient;
    private readonly IShutdownHelperLauncher helperLauncher;

    public AppLifetimeShutdown(IBackgroundIpcClient ipcClient, IShutdownHelperLauncher helperLauncher)
    {
        this.ipcClient = ipcClient;
        this.helperLauncher = helperLauncher;
    }

    public async Task RequestShutdownAllAsync()
    {
        var result = await ipcClient.ShutdownAllAsync().ConfigureAwait(false);
        if (result is { Available: true, Value.Accepted: true })
        {
            return;
        }

        helperLauncher.TryLaunchShutdownAll();
    }
}
