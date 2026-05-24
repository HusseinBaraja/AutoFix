using AutoFix.SettingsUi.Ipc;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class BackgroundAvailabilityMonitor
{
    private const int MissesBeforeClose = 3;
    private readonly IBackgroundIpcClient ipcClient;
    private bool observedRunning;
    private int consecutiveMisses;

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
            consecutiveMisses = 0;
            return false;
        }

        if (!observedRunning)
        {
            return false;
        }

        consecutiveMisses++;
        return consecutiveMisses >= MissesBeforeClose;
    }
}
