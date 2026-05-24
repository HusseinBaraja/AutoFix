using System.IO;
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
        IpcResult<ShutdownAcceptedResponse> result;
        try
        {
            result = await ipcClient.ShutdownAllAsync().ConfigureAwait(false);
        }
        catch (Exception error) when (error is IOException or TimeoutException or OperationCanceledException or InvalidOperationException)
        {
            System.Diagnostics.Debug.WriteLine(error);
            result = IpcResult<ShutdownAcceptedResponse>.Unavailable();
        }

        if (result is { Available: true, Value.Accepted: true })
        {
            return;
        }

        try
        {
            helperLauncher.TryLaunchShutdownAll();
        }
        catch (Exception error) when (error is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            System.Diagnostics.Debug.WriteLine(error);
        }
    }
}
