using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class AppLifetimeShutdownTests
{
    [TestMethod]
    public async Task RequestShutdownAllUsesIpcWhenAvailable()
    {
        var ipc = new FakeIpcClient { ShutdownResult = IpcResult<ShutdownAcceptedResponse>.Ok(new(true)) };
        var helper = new FakeShutdownHelperLauncher();
        var shutdown = new AppLifetimeShutdown(ipc, helper);

        await shutdown.RequestShutdownAllAsync();

        Assert.AreEqual(1, ipc.ShutdownRequests);
        Assert.AreEqual(0, helper.Launches);
    }

    [TestMethod]
    public async Task RequestShutdownAllLaunchesFallbackWhenIpcUnavailable()
    {
        var ipc = new FakeIpcClient { ShutdownResult = IpcResult<ShutdownAcceptedResponse>.Unavailable() };
        var helper = new FakeShutdownHelperLauncher();
        var shutdown = new AppLifetimeShutdown(ipc, helper);

        await shutdown.RequestShutdownAllAsync();

        Assert.AreEqual(1, helper.Launches);
    }

    [TestMethod]
    public async Task RequestShutdownAllDoesNotThrowWhenFallbackLaunchFails()
    {
        var ipc = new FakeIpcClient { ShutdownResult = IpcResult<ShutdownAcceptedResponse>.Unavailable() };
        var helper = new FakeShutdownHelperLauncher { ThrowOnLaunch = true };
        var shutdown = new AppLifetimeShutdown(ipc, helper);

        await shutdown.RequestShutdownAllAsync();

        Assert.AreEqual(1, helper.Launches);
    }

    [TestMethod]
    public async Task RequestShutdownAllUsesFallbackWhenIpcThrows()
    {
        var ipc = new FakeIpcClient { ThrowOnShutdown = true };
        var helper = new FakeShutdownHelperLauncher();
        var shutdown = new AppLifetimeShutdown(ipc, helper);

        await shutdown.RequestShutdownAllAsync();

        Assert.AreEqual(1, helper.Launches);
    }

    [TestMethod]
    public async Task BackgroundMonitorClosesAfterConsecutiveMissesOnlyWhenBackgroundWasObserved()
    {
        var ipc = new FakeIpcClient();
        var monitor = new BackgroundAvailabilityMonitor(ipc);

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Unavailable();
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Ok(new(true));
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Unavailable();
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());
        Assert.IsTrue(await monitor.ShouldCloseSettingsAsync());
    }

    [TestMethod]
    public async Task BackgroundMonitorResetsMissesAfterSuccessfulPoll()
    {
        var ipc = new FakeIpcClient();
        var monitor = new BackgroundAvailabilityMonitor(ipc);

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Ok(new(true));
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Unavailable();
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Ok(new(true));
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());

        ipc.RunningResult = IpcResult<BackgroundRunningResponse>.Unavailable();
        Assert.IsFalse(await monitor.ShouldCloseSettingsAsync());
    }

    private sealed class FakeShutdownHelperLauncher : IShutdownHelperLauncher
    {
        public int Launches { get; private set; }
        public bool ThrowOnLaunch { get; init; }

        public bool TryLaunchShutdownAll()
        {
            Launches++;
            if (ThrowOnLaunch)
            {
                throw new InvalidOperationException("launch failed");
            }

            return true;
        }
    }

    private sealed class FakeIpcClient : IBackgroundIpcClient
    {
        public int ShutdownRequests { get; private set; }
        public bool ThrowOnShutdown { get; init; }
        public IpcResult<ShutdownAcceptedResponse> ShutdownResult { get; set; } =
            IpcResult<ShutdownAcceptedResponse>.Ok(new(true));
        public IpcResult<BackgroundRunningResponse> RunningResult { get; set; } =
            IpcResult<BackgroundRunningResponse>.Ok(new(true));

        public Task<IpcResult<ShutdownAcceptedResponse>> ShutdownAllAsync()
        {
            ShutdownRequests++;
            if (ThrowOnShutdown)
            {
                throw new InvalidOperationException("shutdown failed");
            }

            return Task.FromResult(ShutdownResult);
        }

        public Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync() => Task.FromResult(RunningResult);
        public Task<IpcResult<AppStatusResponse>> GetStatusAsync() => throw new NotImplementedException();
        public Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync() => throw new NotImplementedException();
        public Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync() => throw new NotImplementedException();
        public Task<IpcResult<AppStatusResponse>> ReloadConfigAsync() => throw new NotImplementedException();
        public Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value) => throw new NotImplementedException();
        public Task<IpcResult<AppRulesResponse>> ListAppRulesAsync() => throw new NotImplementedException();
        public Task<IpcResult<AppRuleUpdatedResponse>> UpsertAppRuleAsync(AppRuleDto rule) => throw new NotImplementedException();
        public Task<IpcResult<AppRuleDeletedResponse>> DeleteAppRuleAsync(string processName, string? windowTitlePattern) => throw new NotImplementedException();
        public Task<IpcResult<AppRulesResponse>> ResetAppRulesAsync() => throw new NotImplementedException();
        public Task<IpcResult<LogsResponse>> OpenLogsAsync() => throw new NotImplementedException();
        public Task<IpcResult<CommandAcceptedResponse>> RequestUndoLastCorrectionAsync() => throw new NotImplementedException();
        public Task<IpcResult<CommandAcceptedResponse>> TestCorrectionEngineLaterAsync() => throw new NotImplementedException();
    }
}
