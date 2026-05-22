using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class MainWindowViewModelTests
{
    [TestMethod]
    public async Task SettingChangeSavesConfigAutomatically()
    {
        using var fixture = TempConfig.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());
        await viewModel.LoadSettingsAsync();

        Card(viewModel, "feedback.show_timeout_notice").IsEnabled = false;

        await WaitForAsync(() => !viewModel.IsDirty && ipcClient.ReloadCount > 0);
        var saved = fixture.Storage.Load(fixture.Path);

        Assert.IsFalse(saved.Feedback.ShowTimeoutNotice);
        Assert.AreEqual("Settings saved automatically.", viewModel.StatusTitle);
    }

    private static SettingCardViewModel Card(MainWindowViewModel viewModel, string path) =>
        viewModel.Sections.SelectMany(section => section.Settings).Single(setting => setting.Path == path);

    private static async Task WaitForAsync(Func<bool> done)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!done())
        {
            timeout.Token.ThrowIfCancellationRequested();
            await Task.Delay(20, timeout.Token);
        }
    }

    private sealed class FakeBackgroundIpcClient : IBackgroundIpcClient
    {
        public int ReloadCount { get; private set; }

        public Task<IpcResult<AppStatusResponse>> GetStatusAsync() =>
            Task.FromResult(IpcResult<AppStatusResponse>.Ok(new(true, "typos_only", "local")));

        public Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync() =>
            Task.FromResult(IpcResult<CorrectionModeResponse>.Ok(new("typos_only")));

        public Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync() =>
            Task.FromResult(IpcResult<CorrectionEngineResponse>.Ok(new("local")));

        public Task<IpcResult<AppStatusResponse>> ReloadConfigAsync()
        {
            ReloadCount++;
            return GetStatusAsync();
        }

        public Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value) =>
            Task.FromResult(IpcResult<SettingUpdatedResponse>.Ok(new(path)));

        public Task<IpcResult<LogsResponse>> OpenLogsAsync() =>
            Task.FromResult(IpcResult<LogsResponse>.Ok(new("", true)));

        public Task<IpcResult<CommandAcceptedResponse>> RequestUndoLastCorrectionAsync() =>
            Task.FromResult(IpcResult<CommandAcceptedResponse>.Ok(new(true, "")));

        public Task<IpcResult<CommandAcceptedResponse>> TestCorrectionEngineLaterAsync() =>
            Task.FromResult(IpcResult<CommandAcceptedResponse>.Ok(new(true, "")));

        public Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync() =>
            Task.FromResult(IpcResult<BackgroundRunningResponse>.Ok(new(true)));
    }

    private sealed class NullConfigFileDialog : IConfigFileDialog
    {
        public string? PickImportPath() => null;

        public string? PickExportPath() => null;
    }

    private sealed class TempConfig : IDisposable
    {
        private readonly string root;

        private TempConfig(string root)
        {
            this.root = root;
            Path = System.IO.Path.Combine(root, "settings.toml");
            Storage = new ConfigStorage(Path);
        }

        public string Path { get; }
        public ConfigStorage Storage { get; }

        public static TempConfig Create()
        {
            var root = System.IO.Path.Combine(System.IO.Path.GetTempPath(), $"autofix-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TempConfig(root);
        }

        public void Dispose() => Directory.Delete(root, true);
    }
}
