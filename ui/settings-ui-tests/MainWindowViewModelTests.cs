using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed partial class MainWindowViewModelTests
{
    [TestMethod]
    public void FeedbackWindowRendersQuietDefaultsAndSavesChangedToggle()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                using var fixture = TempConfigFixture.Create();
                var config = AppConfig.Default(); config.Onboarding.Completed = true;
                fixture.Storage.Save(config);
                var viewModel = new MainWindowViewModel(new FakeBackgroundIpcClient(), fixture.Storage,
                    new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite")), new NullConfigFileDialog(), new FakeApiKeyStatus(false), new FakeStartupRegistration());
                window = new MainWindow(viewModel) { ShowActivated = false, Left = -10000, Top = -10000, Width = 1080, Height = 1150 };
                window.Show();
                viewModel.SelectedSection = viewModel.Sections.Single(s => s.Name == "Feedback");
                Pump();
                var boxes = Descendants(window).OfType<System.Windows.Controls.CheckBox>()
                    .Where(box => box.DataContext is SettingCardViewModel).ToArray();
                Assert.AreEqual(7, boxes.Length);
                foreach (var box in boxes)
                {
                    var card = (SettingCardViewModel)box.DataContext;
                    Assert.AreEqual(card.Path is "feedback.tray_state_enabled" or "feedback.show_blocked_app_notice" or "feedback.show_timeout_notice", box.IsChecked);
                }
                var preview = Environment.GetEnvironmentVariable("AUTOFIX_FEEDBACK_PREVIEW");
                if (!string.IsNullOrEmpty(preview))
                {
                    window.UpdateLayout();
                    var bitmap = new System.Windows.Media.Imaging.RenderTargetBitmap((int)window.ActualWidth, (int)window.ActualHeight, 96, 96, System.Windows.Media.PixelFormats.Pbgra32);
                    bitmap.Render(window);
                    var encoder = new System.Windows.Media.Imaging.PngBitmapEncoder();
                    encoder.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(bitmap));
                    using var file = File.Create(preview); encoder.Save(file);
                }
                boxes.Single(box => ((SettingCardViewModel)box.DataContext).Path == "feedback.show_near_caret_overlay")
                    .SetCurrentValue(System.Windows.Controls.Primitives.ToggleButton.IsCheckedProperty, true);
                Pump();
                Assert.IsTrue(fixture.Storage.Load(fixture.Path).Feedback.ShowNearCaretOverlay);
            }
            catch (Exception error) { failure = error; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(15)), "WPF feedback flow timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void DictionaryWindowEditsScopedPairsAndPreservesInvalidEdits()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                using var fixture = TempConfigFixture.Create();
                var config = AppConfig.Default(); config.Onboarding.Completed = true;
                fixture.Storage.Save(config);
                var storage = new DictionaryStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
                var viewModel = new MainWindowViewModel(new FakeBackgroundIpcClient(), fixture.Storage,
                    new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite")), new NullConfigFileDialog(), new FakeApiKeyStatus(false), new FakeStartupRegistration());
                window = new MainWindow(viewModel) { ShowActivated = false, Left = -10000, Top = -10000, Width = 1080, Height = 1150 };
                window.Show();
                viewModel.SelectedSection = viewModel.Sections.Single(s => s.ShowsDictionary);
                viewModel.DictionaryKind = "pair";
                Pump();
                var boxes = Descendants(window).OfType<System.Windows.Controls.TextBox>().ToArray();
                void Type(string name, string text) => boxes.Single(b => System.Windows.Automation.AutomationProperties.GetName(b) == name)
                    .SetCurrentValue(System.Windows.Controls.TextBox.TextProperty, text);
                void Click(string content)
                {
                    var button = Descendants(window).OfType<System.Windows.Controls.Button>().Single(b => Equals(b.Content, content));
                    Assert.IsNotNull(button.Command);
                    button.Command.Execute(button.CommandParameter);
                    Pump();
                }
                Type("Dictionary word or phrase", "teh phrase");
                Type("Blocked replacement", "the phrase");
                Type("Dictionary language", "en-US");
                Type("Dictionary app scope", "Notepad.EXE");
                Click("Save entry");
                var row = storage.List().Single();
                Assert.AreEqual("teh phrase", row.Word);
                Assert.AreEqual("the phrase", row.Replacement);
                Assert.AreEqual("notepad.exe", row.App);
                var grid = Descendants(window).OfType<System.Windows.Controls.DataGrid>().Single(g => System.Windows.Automation.AutomationProperties.GetName(g) == "Dictionary");
                grid.SelectedItem = grid.Items[0];
                Pump();
                Assert.AreEqual("teh phrase", viewModel.DictionaryWord);
                Assert.IsTrue(viewModel.IsPairRule);
                var preview = Environment.GetEnvironmentVariable("AUTOFIX_SETTINGS_PREVIEW");
                if (!string.IsNullOrEmpty(preview))
                {
                    window.UpdateLayout();
                    var bitmap = new System.Windows.Media.Imaging.RenderTargetBitmap((int)window.ActualWidth, (int)window.ActualHeight, 96, 96, System.Windows.Media.PixelFormats.Pbgra32);
                    bitmap.Render(window);
                    var encoder = new System.Windows.Media.Imaging.PngBitmapEncoder();
                    encoder.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(bitmap));
                    using var file = File.Create(preview); encoder.Save(file);
                }
                Type("Dictionary language", "invalid-!");
                Click("Save entry");
                Assert.AreEqual("Exclusion not saved.", viewModel.StatusTitle);
                StringAssert.Contains(viewModel.DictionaryMessage, "BCP 47");
                Assert.AreEqual("en-US", storage.List().Single().Language);
                Type("Dictionary language", "fr");
                Click("Save entry");
                Assert.AreEqual("fr", storage.List().Single().Language);
                grid.SelectedItem = grid.Items[0];
                Pump();
                Click("Delete selected");
                Assert.AreEqual(0, storage.List().Count);
            }
            catch (Exception error) { failure = error; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(15)), "WPF dictionary flow timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    private static void Pump() => System.Windows.Threading.Dispatcher.CurrentDispatcher.Invoke(() => { }, System.Windows.Threading.DispatcherPriority.Background);

    private static IEnumerable<System.Windows.DependencyObject> Descendants(System.Windows.DependencyObject root)
    {
        for (var i = 0; i < System.Windows.Media.VisualTreeHelper.GetChildrenCount(root); i++)
        {
            var child = System.Windows.Media.VisualTreeHelper.GetChild(root, i);
            yield return child;
            foreach (var descendant in Descendants(child)) yield return descendant;
        }
    }

    [TestMethod]
    public async Task SettingChangeSavesConfigAutomatically()
    {
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(
            ipcClient,
            fixture.Storage,
            new NullConfigFileDialog(),
            new FakeApiKeyStatus(false),
            new FakeStartupRegistration());
        await viewModel.LoadSettingsAsync();

        Card(viewModel, "feedback.show_timeout_notice").IsEnabled = false;

        await WaitForAsync(() => !viewModel.IsDirty && ipcClient.ReloadCount > 0);
        var saved = fixture.Storage.Load(fixture.Path);

        Assert.IsFalse(saved.Feedback.ShowTimeoutNotice);
        Assert.AreEqual("Settings saved automatically.", viewModel.StatusTitle);
    }

    /// <summary>Changing correction mode updates category availability and persists only permitted grammar choices.</summary>
    [TestMethod]
    public async Task ModeChangeControlsGrammarAndPersistsEnabledCategories()
    {
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(
            ipcClient, fixture.Storage, new NullConfigFileDialog(),
            new FakeApiKeyStatus(false), new FakeStartupRegistration());
        await viewModel.LoadSettingsAsync();

        var category = Card(viewModel, "correction.enabled_grammar_categories.homophones");
        Assert.IsFalse(category.IsAvailable);
        Card(viewModel, "correction.mode").SelectedValue = "typos_plus_grammar";
        Assert.IsTrue(category.IsAvailable);
        category.IsEnabled = false;
        await WaitForAsync(() => !viewModel.IsDirty && ipcClient.ReloadCount > 0);
        var saved = fixture.Storage.Load(fixture.Path);
        Assert.AreEqual("typos_plus_grammar", saved.Correction.Mode);
        Assert.IsFalse(saved.Correction.EnabledGrammarCategories.Contains("homophones"));
        Assert.IsTrue(saved.Correction.EnabledGrammarCategories.Contains("capitalization"));

        Card(viewModel, "correction.mode").SelectedValue = "typos_only";
        Assert.IsFalse(category.IsAvailable);
        await WaitForAsync(() => fixture.Storage.Load(fixture.Path).Correction.Mode == "typos_only");
        Assert.AreEqual(0, fixture.Storage.Load(fixture.Path).Correction.EnabledGrammarCategories.Count);
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
    public async Task LoadSettingsLoadsAppRules()
    {
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        ipcClient.AppRules.Add(new("code.exe", null, "allowlist", true, false, false, true, true));
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());

        await viewModel.LoadSettingsAsync();

        var section = viewModel.Sections.Single(section => section.ShowsAppRules);
        Assert.AreEqual(1, section.AppRules.Count);
        Assert.AreEqual("code.exe", section.AppRules[0].ProcessName);
    }

    [TestMethod]
    public async Task AddAppRulePersistsThroughIpc()
    {
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());
        await viewModel.LoadSettingsAsync();

        viewModel.NewAppProcess = "app.exe";

        viewModel.AddAppRuleCommand.Execute(null);

        await WaitForAsync(() => ipcClient.UpsertedRules.Count > 0);
        Assert.AreEqual("app.exe", ipcClient.UpsertedRules[0].ProcessName);
    }

    [TestMethod]
    public async Task DeleteAppRulePersistsThroughIpc()
    {
        using var fixture = TempConfigFixture.Create();
        var ipcClient = new FakeBackgroundIpcClient();
        ipcClient.AppRules.Add(new("word.exe", "*admin*", "blocklist", false, false, false, false, false));
        var viewModel = new MainWindowViewModel(ipcClient, fixture.Storage, new NullConfigFileDialog());
        await viewModel.LoadSettingsAsync();
        viewModel.SelectedAppRule = viewModel.Sections.Single(section => section.ShowsAppRules).AppRules[0];

        viewModel.DeleteAppRuleCommand.Execute(null);

        await WaitForAsync(() => ipcClient.DeletedRules.Count > 0);
        Assert.AreEqual("word.exe", ipcClient.DeletedRules[0].ProcessName);
        Assert.AreEqual("*admin*", ipcClient.DeletedRules[0].WindowTitlePattern);
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

    [TestMethod]
    public void SearchSelectsMostLikelyMatchingSection()
    {
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            new ConfigStorage("unused"),
            new NullConfigFileDialog());

        viewModel.SearchText = "timeout";

        Assert.AreEqual("Engines", viewModel.SelectedSection?.Name);
    }

    [TestMethod]
    public void SearchFilterMatchesSettingDescriptionsAndPaths()
    {
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            new ConfigStorage("unused"),
            new NullConfigFileDialog());

        viewModel.SearchText = "fallback_to_local";

        var visibleSections = viewModel.SectionView.Cast<SettingsSectionViewModel>().Select(section => section.Name).ToArray();

        CollectionAssert.AreEqual(new[] { "Engines" }, visibleSections);
        Assert.AreEqual("Engines", viewModel.SelectedSection?.Name);
    }

    [TestMethod]
    public void SearchTextPreservesInputWhileIgnoringSurroundingWhitespaceForMatching()
    {
        var viewModel = new MainWindowViewModel(
            new FakeBackgroundIpcClient(),
            new ConfigStorage("unused"),
            new NullConfigFileDialog());

        viewModel.SearchText = "  fallback_to_local  ";

        var visibleSections = viewModel.SectionView.Cast<SettingsSectionViewModel>().Select(section => section.Name).ToArray();

        Assert.AreEqual("  fallback_to_local  ", viewModel.SearchText);
        CollectionAssert.AreEqual(new[] { "Engines" }, visibleSections);
        Assert.AreEqual("Engines", viewModel.SelectedSection?.Name);
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
        public bool ReloadUnavailable { get; set; }
        public int StatusCheckCount { get; private set; }
        public List<AppRuleDto> AppRules { get; } = [];
        public List<AppRuleDto> UpsertedRules { get; } = [];
        public List<(string ProcessName, string? WindowTitlePattern)> DeletedRules { get; } = [];

        public Task<IpcResult<AppStatusResponse>> GetStatusAsync() =>
            Task.FromResult(IpcResult<AppStatusResponse>.Ok(new(true, "typos_only", "local")));

        public Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync() =>
            Task.FromResult(IpcResult<CorrectionModeResponse>.Ok(new("typos_only")));

        public Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync() =>
            Task.FromResult(IpcResult<CorrectionEngineResponse>.Ok(new("local")));

        public Task<IpcResult<AppStatusResponse>> ReloadConfigAsync()
        {
            ReloadCount++;
            return ReloadUnavailable ? Task.FromResult(IpcResult<AppStatusResponse>.Unavailable()) : GetStatusAsync();
        }

        public Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value) =>
            Task.FromResult(IpcResult<SettingUpdatedResponse>.Ok(new(path)));

        public Task<IpcResult<AppRulesResponse>> ListAppRulesAsync() =>
            Task.FromResult(IpcResult<AppRulesResponse>.Ok(new(AppRules)));

        public Task<IpcResult<AppRuleUpdatedResponse>> UpsertAppRuleAsync(AppRuleDto rule)
        {
            UpsertedRules.Add(rule);
            return Task.FromResult(IpcResult<AppRuleUpdatedResponse>.Ok(new(rule.ProcessName, rule.WindowTitlePattern)));
        }

        public Task<IpcResult<AppRuleDeletedResponse>> DeleteAppRuleAsync(string processName, string? windowTitlePattern)
        {
            DeletedRules.Add((processName, windowTitlePattern));
            return Task.FromResult(IpcResult<AppRuleDeletedResponse>.Ok(new(true)));
        }

        public Task<IpcResult<AppRulesResponse>> ResetAppRulesAsync()
        {
            AppRules.Clear();
            return Task.FromResult(IpcResult<AppRulesResponse>.Ok(new(AppRules)));
        }

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
        public bool FailApplication { get; set; }

        public void Apply(bool startWithWindows)
        {
            ApplyCount++;
            if (FailApplication) throw new InvalidOperationException("Test startup registration failure.");
            StartWithWindows = startWithWindows;
        }
    }
}
