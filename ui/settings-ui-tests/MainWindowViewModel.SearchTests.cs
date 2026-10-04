using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;
using AutoFix.SettingsUi.Controls;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

public sealed partial class MainWindowViewModelTests
{
    [DataTestMethod]
    [DataRow("sign-in", "General", "general.start_with_windows")]
    [DataRow("undo history", "Shortcuts", "context.undo_history_size")]
    [DataRow("word count threshold", "Triggers", "triggers.word_count")]
    [DataRow("high confidence", "Correction", "correction.high_confidence_behavior")]
    [DataRow("AUTOMATIC API TIMEOUT", "Engines", "api.timeout_auto_ms")]
    [DataRow("local fallback", "Engines", "api.fallback_to_local")]
    [DataRow("blocklist mode", "App Rules", "general.run_mode")]
    [DataRow("mixed language policy", "Languages", "correction.mixed_language_policy")]
    [DataRow("clipboard privacy", "Privacy & Security", "replacement.clipboard_enabled")]
    [DataRow("medium suggestions", "Feedback", "feedback.show_medium_confidence_suggestions")]
    [DataRow("logging.log_retention_days", "Logs / Debug", "logging.log_retention_days")]
    [DataRow("pending queue cancel oldest", "Context", "context.pending_queue_full_behavior")]
    public void SearchFindsMetadataAcrossSectionsAndDeepOptions(string query, string sectionName, string path)
    {
        using var fixture = TempConfigFixture.Create();
        var vm = ProductViewModel(fixture, new FakeSettingsConsent());
        vm.SearchText = query;
        Assert.IsTrue(vm.SectionView.Cast<SettingsSectionViewModel>().Any(s => s.Name == sectionName));
        Assert.IsTrue(Card(vm, path).IsSearchMatch);
        Assert.IsFalse(vm.HasNoSearchResults);
    }

    [DataTestMethod]
    [DataRow("API key secure storage", "Engines", "api_key")]
    [DataRow("protected dictionary phrases", "Dictionary", "dictionary")]
    [DataRow("app rules per trigger", "App Rules", "app_rules")]
    [DataRow("refresh metadata logs", "Logs / Debug", "metadata_logs")]
    [DataRow("automatic language detection", "Languages", "language_detection")]
    [DataRow("import export", "Advanced", "")]
    public void SearchFindsCustomEditorsTablesAndActions(string query, string sectionName, string target)
    {
        using var fixture = TempConfigFixture.Create();
        var vm = ProductViewModel(fixture, new FakeSettingsConsent());
        vm.SearchText = query;
        Assert.AreEqual(sectionName, vm.SelectedSection?.Name);
        Assert.IsTrue(vm.SelectedSection!.Settings.Any(c => c.IsSearchMatch && c.SearchTarget == target));
    }

    [TestMethod]
    public async Task SearchFiltersCardsWithoutLosingConfigAndClearRestoresNavigation()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Context.InformativeContextMaxChars = 777;
        config.Api.TimeoutManualMs = 3456;
        fixture.Storage.Save(config);
        var vm = ProductViewModel(fixture, new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        var before = File.ReadAllText(fixture.Path);
        vm.SearchText = "word count threshold";
        Assert.AreEqual("Triggers", vm.SelectedSection?.Name);
        CollectionAssert.AreEqual(new[] { "triggers.word_count" }, vm.SelectedSection!.Settings.Where(c => c.IsSearchVisible).Select(c => c.Path).ToArray());
        Assert.AreEqual(before, File.ReadAllText(fixture.Path), "Search alone must not save config.");
        Card(vm, "triggers.word_count").TextValue = "17";
        await vm.SaveSettingsAsync();
        var saved = fixture.Storage.Load(fixture.Path);
        Assert.AreEqual(17, saved.Triggers.WordCount);
        Assert.AreEqual(777, saved.Context.InformativeContextMaxChars);
        Assert.AreEqual(3456L, saved.Api.TimeoutManualMs);
        vm.SearchText = "unfindable-needle-987";
        Assert.IsNull(vm.SelectedSection);
        Assert.AreEqual(0, vm.SectionView.Cast<SettingsSectionViewModel>().Count());
        Assert.IsTrue(vm.HasNoSearchResults);
        vm.ClearSearchCommand.Execute(null);
        Assert.IsFalse(vm.HasNoSearchResults || vm.HasSearchQuery);
        Assert.IsNotNull(vm.SelectedSection);
        Assert.AreEqual(vm.Sections.Count, vm.SectionView.Cast<SettingsSectionViewModel>().Count());
        Assert.IsTrue(vm.Sections.SelectMany(s => s.Settings).All(c => c.IsSearchMatch));
    }

    [TestMethod]
    public async Task SearchNeverIndexesUserValuesOrPersonalDictionaryEntries()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default(); config.Api.Model = "secretvalue987";
        fixture.Storage.Save(config);
        var vm = ProductViewModel(fixture, new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        vm.DictionaryWord = "secretvalue987"; vm.SaveDictionaryCommand.Execute(null);
        vm.SearchText = "secretvalue987";
        Assert.IsTrue(vm.HasNoSearchResults);
        vm.SearchText = "protected phrases";
        Assert.AreEqual("Dictionary", vm.SelectedSection?.Name);
        Assert.IsTrue(vm.SelectedSection!.ShowDictionaryResults);
    }

    [TestMethod]
    public async Task ActiveSearchSurvivesConfigReloadAndUsesFreshCards()
    {
        using var fixture = TempConfigFixture.Create();
        var vm = ProductViewModel(fixture, new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        vm.SearchText = "api timeout";
        var oldSection = vm.SelectedSection;
        await vm.LoadSettingsAsync();
        Assert.AreEqual("api timeout", vm.SearchText);
        Assert.AreEqual("Engines", vm.SelectedSection?.Name);
        Assert.AreNotSame(oldSection, vm.SelectedSection);
        Assert.IsFalse(Card(vm, "api.temperature").IsSearchMatch);
    }

    [TestMethod]
    public void WpfSearchFiltersHighlightsEditsAndShowsEmptyState()
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
                var vm = ProductViewModel(fixture, new FakeSettingsConsent());
                window = new MainWindow(vm) { ShowActivated = false, Left = -10000, Top = -10000, Width = 1080, Height = 720 };
                window.Show(); Pump();
                var search = (TextBox)window.FindName("SearchBox");
                void Type(string query) { search.SetCurrentValue(TextBox.TextProperty, query); Pump(); }
                search.Focus();
                foreach (var character in "word count threshold") { search.AppendText(character.ToString()); Pump(); }
                Assert.AreEqual("word count threshold", search.Text, "Typing must preserve spaces between query terms.");
                ApplicationCommands.Find.Execute(null, window); Pump();
                Assert.IsTrue(search.IsKeyboardFocusWithin);
                Assert.AreEqual(search.Text.Length, search.SelectionLength);
                Type("pending queue cancel oldest");
                Assert.AreEqual("Context", vm.SelectedSection?.Name);
                var visibleCards = Descendants(window).OfType<HighlightTextBlock>().Where(b => b.IsVisible && b.DataContext is SettingCardViewModel)
                    .Select(b => (SettingCardViewModel)b.DataContext).Distinct().ToArray();
                Assert.AreEqual(1, visibleCards.Length);
                Assert.AreEqual("context.pending_queue_full_behavior", visibleCards[0].Path);
                Assert.IsTrue(Descendants(window).OfType<HighlightTextBlock>().Where(b => b.IsVisible)
                    .SelectMany(b => b.Inlines.OfType<Run>()).Any(run => Equals(run.Background, Brushes.Gold)));
                Assert.AreEqual(0, ((ScrollViewer)window.FindName("SettingsScrollViewer")).VerticalOffset);
                SavePreview(window, "Search-queue", "results");
                Type("word count threshold");
                var threshold = Descendants(window).OfType<TextBox>().Single(box => box.IsVisible && box.DataContext is SettingCardViewModel { Path: "triggers.word_count" });
                threshold.SetCurrentValue(TextBox.TextProperty, "23"); Pump();
                Assert.AreEqual(23, fixture.Storage.Load(fixture.Path).Triggers.WordCount);
                SavePreview(window, "Search-threshold", "results");
                Type("API key secure storage");
                Assert.IsTrue(((PasswordBox)window.FindName("ApiKeyBox")).IsVisible);
                Assert.IsFalse(Descendants(window).OfType<TextBox>().Any(box => box.IsVisible && box.DataContext is SettingCardViewModel));
                SavePreview(window, "Search-api-key", "results");
                Type("impossible-search-987");
                Assert.IsNull(vm.SelectedSection);
                Assert.IsFalse(((PasswordBox)window.FindName("ApiKeyBox")).IsVisible);
                Assert.IsFalse(((ScrollViewer)window.FindName("SettingsScrollViewer")).IsVisible);
                Assert.IsTrue(Descendants(window).OfType<TextBlock>().Any(b => b.IsVisible && b.Text == "No matching settings"));
                SavePreview(window, "Search-empty", "results");
                Descendants(window).OfType<Button>().Single(b => Equals(b.Content, "Clear search")).Command.Execute(null); Pump();
                Assert.AreEqual("", search.Text);
                Assert.AreEqual(vm.Sections.Count, vm.SectionView.Cast<SettingsSectionViewModel>().Count());
                Type("clipboard privacy");
                Assert.AreEqual("Privacy & Security", vm.SelectedSection?.Name);
                Assert.AreEqual(1, vm.SelectedSection!.Settings.Count(card => card.IsSearchVisible));
                SavePreview(window, "Search-privacy", "results");
                // Esc is handled through the window's real preview key route while search has focus.
                search.Focus();
                var keyEvent = new KeyEventArgs(Keyboard.PrimaryDevice, PresentationSource.FromVisual(window), 0, Key.Escape)
                    { RoutedEvent = Keyboard.PreviewKeyDownEvent };
                search.RaiseEvent(keyEvent); Pump();
                Assert.IsTrue(keyEvent.Handled);
                Assert.AreEqual("", vm.SearchText);
            }
            catch (Exception error) { failure = error; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA); thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(25)), "WPF search flow timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void HighlightMergesOverlappingTermsAndRestoresTextWhenCleared()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try
            {
                var block = new HighlightTextBlock { HighlightText = "queue queueing — AR العربية", Query = "queue queueing العربية" };
                Assert.AreEqual(block.HighlightText, new TextRange(block.ContentStart, block.ContentEnd).Text);
                var highlighted = block.Inlines.OfType<Run>().Where(run => Equals(run.Background, Brushes.Gold)).Select(run => run.Text).ToArray();
                CollectionAssert.AreEqual(new[] { "queue", "queueing", "العربية" }, highlighted);
                block.Query = "";
                Assert.AreEqual(block.HighlightText, block.Inlines.OfType<Run>().Single().Text);
                Assert.IsFalse(Equals(block.Inlines.OfType<Run>().Single().Background, Brushes.Gold));
            }
            catch (Exception error) { failure = error; }
        });
        thread.SetApartmentState(ApartmentState.STA); thread.Start(); thread.Join();
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }
}
