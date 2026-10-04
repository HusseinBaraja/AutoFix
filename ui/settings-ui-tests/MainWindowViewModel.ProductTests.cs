using System.Security;
using System.Windows;
using System.Windows.Controls;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

public sealed partial class MainWindowViewModelTests
{
    /// <summary>Invalid dropdown profiles show a recoverable credential message on change, refresh and navigation.</summary>
    [DataTestMethod]
    [DataRow("")]
    [DataRow(" ")]
    [DataRow("bad\nprofile")]
    public async Task InvalidProviderProfileDoesNotInterruptSettings(string profile)
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        var vm = new MainWindowViewModel(new FakeBackgroundIpcClient(), fixture.Storage, storage,
            new NullConfigFileDialog(), new WindowsCredentialApiKeyStatus(), new FakeStartupRegistration(), new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        Card(vm, "api.provider_preset").SelectedValue = profile;
        StringAssert.Contains(vm.ApiKeyMessage, "Choose a valid provider profile.");
        vm.RefreshApiKeyCommand.Execute(null);
        vm.SelectedSection = vm.Sections.Single(section => section.Settings.Any(card => card.Path == "api.provider_preset"));
        StringAssert.Contains(vm.ApiKeyMessage, "Choose a valid provider profile.");
        Assert.AreEqual("openai_compatible", fixture.Storage.Load(fixture.Path).Api.ProviderPreset);
        Card(vm, "api.provider_preset").SelectedValue = "openai_compatible";
        Assert.IsFalse(vm.ApiKeyMessage.Contains("Choose a valid provider profile."));
    }

    [TestMethod]
    public async Task DebugConsentAndDependenciesSaveValidConfigs()
    {
        using var fixture = TempConfigFixture.Create();
        var consent = new FakeSettingsConsent();
        var vm = ProductViewModel(fixture, consent);
        await vm.LoadSettingsAsync();
        Card(vm, "logging.full_text_debug_mode_enabled").IsEnabled = true;
        Assert.AreEqual(1, consent.FullTextRequests);
        Assert.IsFalse(Card(vm, "logging.full_text_debug_mode_enabled").IsEnabled);
        Assert.IsFalse(fixture.Storage.Load(fixture.Path).Logging.FullTextDebugModeEnabled);

        Card(vm, "logging.redacted_debug_mode_enabled").IsEnabled = true;
        await vm.SaveSettingsAsync();
        var saved = fixture.Storage.Load(fixture.Path).Logging;
        Assert.IsTrue(saved.DebugModeEnabled && saved.RedactedDebugModeEnabled);
        Assert.IsFalse(saved.FullTextDebugModeEnabled);

        consent.AllowFullText = true;
        Card(vm, "logging.full_text_debug_mode_enabled").IsEnabled = true;
        await vm.SaveSettingsAsync();
        saved = fixture.Storage.Load(fixture.Path).Logging;
        Assert.IsTrue(saved.DebugModeEnabled && saved.FullTextDebugModeEnabled);
        Assert.IsFalse(saved.RedactedDebugModeEnabled);

        Card(vm, "logging.debug_mode_enabled").IsEnabled = false;
        await vm.SaveSettingsAsync();
        saved = fixture.Storage.Load(fixture.Path).Logging;
        Assert.IsFalse(saved.DebugModeEnabled || saved.RedactedDebugModeEnabled || saved.FullTextDebugModeEnabled);
    }

    [TestMethod]
    public async Task ImportRequiresConsentBeforeEnablingFullTextAndKeepsAppRules()
    {
        using var fixture = TempConfigFixture.Create();
        var importedPath = Path.Combine(fixture.Root, "import.toml");
        var imported = AppConfig.Default();
        imported.Logging.DebugModeEnabled = true;
        imported.Logging.FullTextDebugModeEnabled = true;
        fixture.Storage.Export(importedPath, imported);
        var consent = new FakeSettingsConsent();
        var ipc = new FakeBackgroundIpcClient();
        ipc.AppRules.Add(new("notepad.exe", null, "allowlist", true, false, false, true, false));
        var vm = new MainWindowViewModel(ipc, fixture.Storage, new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite")),
            new ImportOnlyDialog(importedPath), new FakeApiKeyStore(), new FakeStartupRegistration(), consent);
        await vm.LoadSettingsAsync();
        var before = File.ReadAllText(fixture.Path);
        vm.ImportConfigCommand.Execute(null);
        Assert.IsTrue(vm.IsImportPreviewVisible);
        Assert.AreEqual(0, consent.FullTextRequests);
        vm.ConfirmImportCommand.Execute(null);
        await WaitForAsync(() => consent.FullTextRequests == 1);
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.IsFalse(Card(vm, "logging.full_text_debug_mode_enabled").IsEnabled);
        consent.AllowFullText = true;
        vm.ImportConfigCommand.Execute(null);
        vm.ConfirmImportCommand.Execute(null);
        await WaitForAsync(() => vm.StatusTitle == "Settings imported.");
        Assert.IsTrue(fixture.Storage.Load(fixture.Path).Logging.FullTextDebugModeEnabled);
        Assert.AreEqual("notepad.exe", vm.Sections.Single(s => s.ShowsAppRules).AppRules.Single().ProcessName);
    }

    [TestMethod]
    public async Task ProviderChangeClearsCustomEndpointAndKeysStayOutsideConfig()
    {
        using var fixture = TempConfigFixture.Create();
        var keys = new FakeApiKeyStore();
        var config = AppConfig.Default();
        config.Api.ProviderPreset = "custom";
        config.Api.BaseUrl = "https://example.com/v1";
        fixture.Storage.Save(config);
        var vm = ProductViewModel(fixture, new FakeSettingsConsent(), keys);
        await vm.LoadSettingsAsync();
        using var secret = new SecureString();
        foreach (var c in "dummy-key-must-not-export") secret.AppendChar(c);
        vm.SaveApiKey(secret);
        Assert.AreEqual("custom", keys.SavedProfile);
        Assert.IsTrue(keys.Stored.Contains("custom"));
        Assert.IsFalse(File.ReadAllText(fixture.Path).Contains("dummy-key-must-not-export"));
        Card(vm, "api.provider_preset").SelectedValue = "groq";
        await vm.SaveSettingsAsync();
        Assert.IsNull(fixture.Storage.Load(fixture.Path).Api.BaseUrl);
        StringAssert.Contains(vm.ApiKeyMessage, "No key saved for groq");
        vm.SaveApiKey(secret);
        vm.DeleteApiKeyCommand.Execute(null);
        Assert.IsFalse(keys.Stored.Contains("groq"));
        Assert.IsTrue(keys.Stored.Contains("custom"));
    }

    [TestMethod]
    public async Task AppRuleDraftValidationAndTriggerPermissionsPersistThroughIpc()
    {
        using var fixture = TempConfigFixture.Create();
        var ipc = new FakeBackgroundIpcClient();
        var vm = new MainWindowViewModel(ipc, fixture.Storage, new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite")),
            new NullConfigFileDialog(), new FakeApiKeyStore(), new FakeStartupRegistration(), new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        vm.NewAppProcess = @"C:\notes.exe";
        vm.AddAppRuleCommand.Execute(null);
        Assert.AreEqual(0, ipc.UpsertedRules.Count);
        vm.NewAppProcess = "notes.exe";
        vm.AddAppRuleCommand.Execute(null);
        await WaitForAsync(() => ipc.UpsertedRules.Count == 1);
        var rule = vm.SelectedAppRule!;
        rule.ManualShortcutAllowed = true;
        rule.WordCountTriggerAllowed = true;
        rule.CharacterTriggerAllowed = true;
        rule.ApiEngineAllowed = false;
        await WaitForAsync(() => ipc.UpsertedRules.Count == 5);
        var saved = ipc.UpsertedRules.Last();
        Assert.IsTrue(saved.ManualShortcutAllowed && saved.WordCountTriggerAllowed && saved.CharacterTriggerAllowed);
        Assert.IsTrue(saved.LocalEngineAllowed);
        Assert.IsFalse(saved.ApiEngineAllowed);
        vm.NewAppProcess = "NOTES.EXE";
        vm.AddAppRuleCommand.Execute(null);
        Assert.AreEqual(1, vm.Sections.Single(s => s.ShowsAppRules).AppRules.Count);
        Assert.AreEqual(5, ipc.UpsertedRules.Count);
        Card(vm, "general.run_mode").SelectedValue = "allowlist";
        await vm.SaveSettingsAsync();
        Assert.AreEqual("allowlist", fixture.Storage.Load(fixture.Path).General.RunMode);
    }

    [TestMethod]
    public async Task LanguageDefaultsAndOverridesRoundTripWithoutDuplicatePaths()
    {
        using var fixture = TempConfigFixture.Create();
        var vm = ProductViewModel(fixture, new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        StringAssert.Contains(vm.LanguageDetectionSummary, "Automatic");
        Card(vm, "correction.preferred_language").TextValue = "ar";
        Card(vm, "correction.app_language_overrides").TextValue = "notepad.exe=en-US, winword.exe=fr";
        Card(vm, "correction.mixed_language_policy").SelectedValue = "disable_correction";
        await vm.SaveSettingsAsync();
        var saved = fixture.Storage.Load(fixture.Path);
        Assert.AreEqual("ar", saved.Correction.PreferredLanguage);
        CollectionAssert.AreEqual(new[] { "notepad.exe=en-US", "winword.exe=fr" }, saved.Correction.AppLanguageOverrides);
        vm.AutoDetectLanguageCommand.Execute(null);
        await vm.SaveSettingsAsync();
        saved = fixture.Storage.Load(fixture.Path);
        Assert.IsNull(saved.Correction.PreferredLanguage);
        Assert.AreEqual(2, saved.Correction.AppLanguageOverrides.Count);
        Assert.AreEqual("disable_correction", saved.Correction.MixedLanguagePolicy);
        Assert.AreEqual("App Rules", vm.Sections.Single(s => s.Settings.Any(c => c.Path == "general.run_mode")).Name);
        Card(vm, "correction.app_language_overrides").TextValue = "notepad.exe=bad_tag";
        await vm.SaveSettingsAsync();
        Assert.IsTrue(Card(vm, "correction.app_language_overrides").HasValidationError);
        Assert.AreEqual(2, fixture.Storage.Load(fixture.Path).Correction.AppLanguageOverrides.Count);
    }

    [TestMethod]
    public void ProductPagesRenderAndKeyEditorClearsAcrossProfilesAndNavigation()
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
                var dictionaries = new DictionaryStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
                dictionaries.Save(new() { Word = "AutoFix", Language = "und", Source = "dictionary" });
                dictionaries.Save(new() { Word = "projet secret", Language = "fr", App = "notepad.exe", Source = "dictionary" });
                var keys = new FakeApiKeyStore();
                var consent = new FakeSettingsConsent();
                var vm = ProductViewModel(fixture, consent, keys);
                window = new MainWindow(vm) { ShowActivated = false, Left = -10000, Top = -10000, Width = 1080, Height = 720 };
                window.Show(); Pump();
                var scroll = (ScrollViewer)window.FindName("SettingsScrollViewer");
                foreach (var name in new[] { "App Rules", "Dictionary", "Engines", "Logs / Debug", "Languages" })
                {
                    vm.SelectedSection = vm.Sections.Single(s => s.Name == name);
                    Pump(); scroll.ScrollToTop(); Pump();
                    Assert.IsTrue(Descendants(window).OfType<TextBlock>().Any(b => b.IsVisible));
                    SavePreview(window, name, "top");
                    scroll.ScrollToBottom(); Pump();
                    SavePreview(window, name, "bottom");
                }
                vm.SelectedSection = vm.Sections.Single(s => s.ShowsEngines); Pump(); scroll.ScrollToBottom(); Pump();
                var password = (PasswordBox)window.FindName("ApiKeyBox");
                Assert.IsTrue(password.IsVisible);
                password.Password = "test-key";
                Descendants(window).OfType<Button>().Single(b => Equals(b.Content, "Save key securely"))
                    .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.AreEqual("openai_compatible", keys.SavedProfile);
                Assert.AreEqual(0, password.SecurePassword.Length);
                password.Password = "pending-key";
                var provider = Descendants(window).OfType<ComboBox>().Single(b => b.IsVisible && b.DataContext is SettingCardViewModel { Path: "api.provider_preset" });
                provider.SetCurrentValue(System.Windows.Controls.Primitives.Selector.SelectedValueProperty, "deepseek"); Pump();
                Assert.AreEqual(0, password.SecurePassword.Length);
                password.Password = "pending-key";
                vm.SelectedSection = vm.Sections.Single(s => s.ShowsLogs); Pump();
                Assert.AreEqual(0, password.SecurePassword.Length);
                var full = Descendants(window).OfType<CheckBox>().Single(b => b.IsVisible && b.DataContext is SettingCardViewModel { Path: "logging.full_text_debug_mode_enabled" });
                full.SetCurrentValue(System.Windows.Controls.Primitives.ToggleButton.IsCheckedProperty, true); Pump();
                Assert.AreEqual(1, consent.FullTextRequests);
                Assert.IsFalse(full.IsChecked == true);
                Assert.IsFalse(fixture.Storage.Load(fixture.Path).Logging.FullTextDebugModeEnabled);
                vm.SelectedSection = vm.Sections.Single(s => s.ShowsDictionary); Pump();
                vm.NewDictionaryCommand.Execute(null);
                vm.DictionaryWord = "brand name"; vm.DictionaryLanguage = "en-US"; vm.DictionaryApp = "winword.exe";
                vm.SaveDictionaryCommand.Execute(null); Pump();
                Assert.IsTrue(dictionaries.List().Any(d => d.Word == "brand name" && d.Language == "en-US" && d.App == "winword.exe"));
                vm.SelectedDictionary = vm.Sections.Single(s => s.ShowsDictionary).Dictionary.Single(d => d.Word == "brand name");
                vm.DeleteDictionaryCommand.Execute(null); Pump();
                Assert.IsFalse(dictionaries.List().Any(d => d.Word == "brand name"));
                vm.NewAppProcess = "notes.exe";
                vm.AddAppRuleCommand.Execute(null); Pump();
                var rule = vm.SelectedAppRule!;
                rule.ManualShortcutAllowed = true; rule.WordCountTriggerAllowed = true;
                rule.CharacterTriggerAllowed = true; rule.ApiEngineAllowed = false;
                vm.SelectedSection = vm.Sections.Single(s => s.ShowsAppRules); Pump();
                var grid = Descendants(window).OfType<DataGrid>().Single(g => g.IsVisible);
                Assert.AreEqual(rule, grid.SelectedItem);
                Assert.IsTrue(grid.Columns.Take(2).All(column => column.IsReadOnly));
                vm.ClearLogsCommand.Execute(null);
                Assert.AreEqual(1, consent.ClearRequests);
                consent.AllowClear = true;
                vm.ClearLogsCommand.Execute(null);
                StringAssert.Contains(vm.LogsMessage, "cleared");
            }
            catch (Exception error) { failure = error; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA); thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(25)), "WPF settings flow timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    private static void SavePreview(MainWindow window, string section, string position)
    {
        var directory = Environment.GetEnvironmentVariable("AUTOFIX_SETTINGS_PREVIEW_DIR");
        if (string.IsNullOrEmpty(directory)) return;
        Directory.CreateDirectory(directory);
        window.UpdateLayout();
        var bitmap = new System.Windows.Media.Imaging.RenderTargetBitmap((int)window.ActualWidth, (int)window.ActualHeight,
            96, 96, System.Windows.Media.PixelFormats.Pbgra32);
        bitmap.Render(window);
        var encoder = new System.Windows.Media.Imaging.PngBitmapEncoder();
        encoder.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(bitmap));
        using var file = File.Create(Path.Combine(directory, section.Replace(" / ", "-").Replace(" ", "-") + "-" + position + ".png"));
        encoder.Save(file);
    }

    private static MainWindowViewModel ProductViewModel(TempConfigFixture fixture, FakeSettingsConsent consent, FakeApiKeyStore? keys = null)
    {
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        var ipc = new FakeBackgroundIpcClient();
        ipc.AppRules.AddRange(storage.List().Select(AppRuleStorage.ToDto));
        return new(ipc, fixture.Storage, storage, new NullConfigFileDialog(), keys ?? new FakeApiKeyStore(), new FakeStartupRegistration(), consent);
    }

    private sealed class FakeSettingsConsent : ISettingsConsent
    {
        public bool AllowFullText { get; set; }
        public bool AllowClear { get; set; }
        public int FullTextRequests { get; private set; }
        public int ClearRequests { get; private set; }
        public bool ConfirmFullTextDebug() { FullTextRequests++; return AllowFullText; }
        public bool ConfirmClearLogs() { ClearRequests++; return AllowClear; }
    }

    private sealed class FakeApiKeyStore : IApiKeyStore
    {
        public HashSet<string> Stored { get; } = [];
        public string? SavedProfile { get; private set; }
        public bool HasConfiguredApiKey(AppConfig config) => Stored.Contains(config.Api.ProviderPreset);
        public void Save(string profile, SecureString key) { Assert.IsTrue(key.Length > 0); Stored.Add(profile); SavedProfile = profile; }
        public void Delete(string profile) => Stored.Remove(profile);
    }

    private sealed class ImportOnlyDialog(string path) : IConfigFileDialog
    {
        public string? PickImportPath() => path;
        public string? PickExportPath() => null;
    }
}
