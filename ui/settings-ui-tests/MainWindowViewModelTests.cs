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
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());
        await viewModel.LoadSettingsAsync();

        Card(viewModel, "feedback.show_timeout_notice").IsEnabled = false;

        await WaitForAsync(() => !viewModel.IsDirty && ipcClient.ReloadCount > 0);
        var saved = fixture.Storage.Load(fixture.Path);

        Assert.IsFalse(saved.Feedback.ShowTimeoutNotice);
        Assert.AreEqual("Settings saved automatically.", viewModel.StatusTitle);
    }

    [TestMethod]
    public async Task LoadSettingsDetachesOldSettingHandlers()
    {
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());
        var oldCard = Card(viewModel, "feedback.show_timeout_notice");

        await viewModel.LoadSettingsAsync();

        oldCard.IsEnabled = false;
        await Task.Delay(100);

        Assert.IsFalse(viewModel.IsDirty);
        Assert.AreEqual(0, ipcClient.ReloadCount);
    }

    [TestMethod]
    public async Task ExportFailureDoesNotMarkValidationErrors()
    {
        using var fixture = TempConfigFixture.Create();
        var exportPath = Path.Combine(fixture.Root, "export.toml");
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            fixture.Storage,
            new ExportConfigFileDialog(exportPath));
        await viewModel.LoadSettingsAsync();

        Card(viewModel, "api.timeout_manual_ms").TextValue = "invalid";
        await WaitForAsync(() => viewModel.StatusTitle == "Settings not saved.");
        ConfigFormMapper.ClearValidation(viewModel.Sections);

        viewModel.ExportConfigCommand.Execute(null);

        await WaitForAsync(() => viewModel.StatusTitle == "Export failed.");

        Assert.IsFalse(viewModel.Sections.SelectMany(section => section.Settings).Any(setting => setting.HasValidationError));
        Assert.IsTrue(viewModel.StatusDetail.Contains("api.timeout_manual_ms", StringComparison.Ordinal));
    }

    [TestMethod]
    public async Task LoadFailureKeepsConfigErrorStatus()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Api.TimeoutManualMs = 0;
        await File.WriteAllTextAsync(fixture.Path, ConfigStorage.ToToml(config));
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());

        await viewModel.LoadSettingsAsync();

        Assert.AreEqual("Settings load failed.", viewModel.StatusTitle);
        Assert.IsTrue(viewModel.StatusDetail.Contains("api.timeout_manual_ms", StringComparison.Ordinal));
        Assert.AreEqual(0, ipcClient.StatusCheckCount);
    }

    [TestMethod]
    public async Task NewConfigShowsProgressiveOnboardingWithSafeDefaults()
    {
        using var fixture = TempConfigFixture.Create();
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            fixture.Storage,
            new NullConfigFileDialog(),
            new FakeApiKeyStatus(false),
            new FakeStartupRegistration());

        await viewModel.LoadSettingsAsync();

        Assert.IsTrue(viewModel.IsOnboardingVisible);
        Assert.IsNotNull(viewModel.Onboarding);
        Assert.AreEqual("Ctrl+Alt+Space", viewModel.Onboarding.Shortcut);
        Assert.AreEqual("typos_only", viewModel.Onboarding.Mode);
        Assert.AreEqual("local", viewModel.Onboarding.Engine);
        Assert.AreEqual(OnboardingViewModel.UseLocalForNow, viewModel.Onboarding.ApiWithoutKeyChoice);
        Assert.IsTrue(viewModel.Onboarding.StartWithWindows);
    }

    [TestMethod]
    public async Task ApiWithoutKeyUsesLocalEngineForNowByDefault()
    {
        using var fixture = TempConfigFixture.Create();
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            fixture.Storage,
            new NullConfigFileDialog(),
            new FakeApiKeyStatus(false),
            new FakeStartupRegistration());
        await viewModel.LoadSettingsAsync();
        viewModel.Onboarding!.Engine = "api";

        viewModel.Onboarding.FinishCommand.Execute(null);

        var saved = fixture.Storage.Load(fixture.Path);
        Assert.IsFalse(viewModel.IsOnboardingVisible);
        Assert.IsTrue(saved.Onboarding.Completed);
        Assert.IsTrue(saved.Correction.Enabled);
        Assert.AreEqual("local", saved.Correction.Engine);
        Assert.IsTrue(saved.General.StartWithWindows);
    }

    [TestMethod]
    public async Task CompletingOnboardingRegistersCurrentUserStartup()
    {
        using var fixture = TempConfigFixture.Create();
        var startupRegistration = new FakeStartupRegistration();
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            fixture.Storage,
            new NullConfigFileDialog(),
            new FakeApiKeyStatus(false),
            startupRegistration);
        await viewModel.LoadSettingsAsync();

        viewModel.Onboarding!.FinishCommand.Execute(null);

        Assert.AreEqual(1, startupRegistration.ApplyCount);
        Assert.IsTrue(startupRegistration.StartWithWindows);
    }

    [TestMethod]
    public async Task SettingChangeUpdatesCurrentUserStartup()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Onboarding.Completed = true;
        config.General.StartWithWindows = true;
        fixture.Storage.Save(config);
        var startupRegistration = new FakeStartupRegistration();
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            fixture.Storage,
            new NullConfigFileDialog(),
            new FakeApiKeyStatus(false),
            startupRegistration);
        await viewModel.LoadSettingsAsync();

        Card(viewModel, "general.start_with_windows").IsEnabled = false;

        await WaitForAsync(() => startupRegistration.ApplyCount > 0);
        Assert.IsFalse(startupRegistration.StartWithWindows);
    }

    [TestMethod]
    public async Task ApiWithoutKeyCanFinishWithCorrectionDisabled()
    {
        using var fixture = TempConfigFixture.Create();
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            fixture.Storage,
            new NullConfigFileDialog(),
            new FakeApiKeyStatus(false),
            new FakeStartupRegistration());
        await viewModel.LoadSettingsAsync();
        viewModel.Onboarding!.Engine = "api";
        viewModel.Onboarding.ApiWithoutKeyChoice = OnboardingViewModel.DisableUntilConfigured;

        viewModel.Onboarding.FinishCommand.Execute(null);

        var saved = fixture.Storage.Load(fixture.Path);
        Assert.IsTrue(saved.Onboarding.Completed);
        Assert.IsFalse(saved.Correction.Enabled);
        Assert.AreEqual("api", saved.Correction.Engine);
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
        public int StatusCheckCount { get; private set; }

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

        public Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync()
        {
            StatusCheckCount++;
            return Task.FromResult(IpcResult<BackgroundRunningResponse>.Ok(new(true)));
        }
    }

    private sealed class NullConfigFileDialog : IConfigFileDialog
    {
        public string? PickImportPath() => null;

        public string? PickExportPath() => null;
    }

    private sealed class ExportConfigFileDialog(string exportPath) : IConfigFileDialog
    {
        public string? PickImportPath() => null;

        public string? PickExportPath() => exportPath;
    }

    private sealed class FakeApiKeyStatus(bool hasKey) : IApiKeyStatus
    {
        public bool HasConfiguredApiKey(AppConfig config) => hasKey;
    }

    private sealed class FakeStartupRegistration : IStartupRegistration
    {
        public int ApplyCount { get; private set; }
        public bool StartWithWindows { get; private set; }

        public void Apply(bool startWithWindows)
        {
            ApplyCount++;
            StartWithWindows = startWithWindows;
        }
    }
}
