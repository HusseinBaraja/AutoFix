using AutoFix.SettingsUi.Ipc;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class BackgroundAvailabilityMonitor
{
    private readonly IBackgroundIpcClient ipcClient;
    private bool observedRunning;

    public BackgroundAvailabilityMonitor(IBackgroundIpcClient ipcClient)
    {
        this.ipcClient = ipcClient;
    }

    public async Task<bool> ShouldCloseSettingsAsync()
    {
        var result = await ipcClient.IsBackgroundRunningAsync().ConfigureAwait(false);
        if (result is { Available: true, Value.Running: true })
        {
            observedRunning = true;
            return false;
        }

        return observedRunning;
    }
}
