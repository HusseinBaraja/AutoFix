using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ConfigFormMapperTests
{
    [TestMethod]
    public void FeedbackHasQuietDefaultsAndEveryOptionSurvivesFormAndStorage()
    {
        using var fixture = TempConfigFixture.Create();
        var sections = SettingsSkeleton.CreateSections();
        foreach (var option in new[] { "tray_state_enabled", "show_correction_applied_notification", "show_skipped_reason", "show_medium_confidence_suggestions", "show_blocked_app_notice", "show_timeout_notice", "show_near_caret_overlay" })
        {
            var card = Card(sections, $"feedback.{option}");
            Assert.AreEqual(option is "tray_state_enabled" or "show_blocked_app_notice" or "show_timeout_notice", card.IsEnabled);
            card.IsEnabled = !card.IsEnabled;
        }
        var config = ConfigFormMapper.BuildConfig(sections);
        fixture.Storage.Save(config);
        var loaded = fixture.Storage.Load(fixture.Path);
        var reloaded = SettingsSkeleton.CreateSections(loaded);
        foreach (var card in sections.Single(section => section.Name == "Feedback").Settings)
            Assert.AreEqual(card.IsEnabled, Card(reloaded, card.Path).IsEnabled, card.Path);
        File.WriteAllLines(fixture.Path, File.ReadAllLines(fixture.Path).Where(line => !line.StartsWith("show_near_caret_overlay", StringComparison.Ordinal)));
        Assert.IsFalse(fixture.Storage.Load(fixture.Path).Feedback.ShowNearCaretOverlay);
    }

    [TestMethod]
    public void UndoShortcutAndHistorySurviveFormStorageAndLegacyLoad()
    {
        using var fixture = TempConfigFixture.Create();
        var sections = SettingsSkeleton.CreateSections();
        Assert.AreEqual("Ctrl+Alt+Z", Card(sections, "shortcuts.undo").Hotkey);
        Assert.AreEqual("10", Card(sections, "context.undo_history_size").TextValue);
        Card(sections, "shortcuts.undo").Hotkey = "Ctrl+Shift+Z";
        foreach (var size in new[] { 1, 25, 1000 })
        {
            Card(sections, "context.undo_history_size").TextValue = size.ToString();
            fixture.Storage.Save(ConfigFormMapper.BuildConfig(sections));
            var loaded = fixture.Storage.Load(fixture.Path);
            Assert.AreEqual(size, loaded.Context.UndoHistorySize);
            Assert.AreEqual("Ctrl+Shift+Z", loaded.Shortcuts.Undo);
            Assert.AreEqual(size.ToString(), Card(SettingsSkeleton.CreateSections(loaded), "context.undo_history_size").TextValue);
        }
        var legacy = File.ReadAllLines(fixture.Path).Where(line => !line.StartsWith("undo_history_size", StringComparison.Ordinal));
        File.WriteAllLines(fixture.Path, legacy);
        Assert.AreEqual(10, fixture.Storage.Load(fixture.Path).Context.UndoHistorySize);
        foreach (var size in new[] { "0", "1001" })
        {
            Card(sections, "context.undo_history_size").TextValue = size;
            var error = Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));
            Assert.AreEqual("context.undo_history_size: must be between 1 and 1000", error.Message);
        }
    }

    [TestMethod]
    public void ClipboardPreferenceSurvivesFormAndStorageRoundTrip()
    {
        using var fixture = TempConfigFixture.Create();
        var sections = SettingsSkeleton.CreateSections();
        Assert.IsTrue(Card(sections, "replacement.clipboard_enabled").IsEnabled);
        Card(sections, "replacement.clipboard_enabled").IsEnabled = false;
        var config = ConfigFormMapper.BuildConfig(sections);
        Assert.IsFalse(config.Replacement.ClipboardEnabled);
        fixture.Storage.Save(config);
        var loaded = fixture.Storage.Load(fixture.Path);
        Assert.IsFalse(loaded.Replacement.ClipboardEnabled);
        Assert.IsFalse(Card(SettingsSkeleton.CreateSections(loaded), "replacement.clipboard_enabled").IsEnabled);
        var legacy = File.ReadAllLines(fixture.Path).Where(line => !line.StartsWith("clipboard_enabled", StringComparison.Ordinal));
        File.WriteAllLines(fixture.Path, legacy);
        Assert.IsTrue(fixture.Storage.Load(fixture.Path).Replacement.ClipboardEnabled);
    }

    [TestMethod]
    public void PendingQueueChoicesMapAndRejectInvalidCapacityOrPolicy()
    {
        var sections = SettingsSkeleton.CreateSections();
        Assert.AreEqual("1", Card(sections, "context.pending_queue_size").TextValue);
        Assert.AreEqual("skip_new", Card(sections, "context.pending_queue_full_behavior").SelectedValue);
        foreach (var policy in new[] { "skip_new", "cancel_oldest", "merge_newest" })
        {
            Card(sections, "context.pending_queue_size").TextValue = "4";
            Card(sections, "context.pending_queue_full_behavior").SelectedValue = policy;
            var config = ConfigFormMapper.BuildConfig(sections);
            Assert.AreEqual(4, config.Context.PendingQueueSize);
            Assert.AreEqual(policy, config.Context.PendingQueueFullBehavior);
        }
        foreach (var size in new[] { "0", "17" })
        {
            Card(sections, "context.pending_queue_size").TextValue = size;
            Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));
        }
        Card(sections, "context.pending_queue_size").TextValue = "1";
        Card(sections, "context.pending_queue_full_behavior").SelectedValue = "unknown";
        Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));
    }

    /// <summary>Form confidence choices persist while unsafe low-confidence behavior is rejected.</summary>
    [TestMethod]
    public void ConfidenceChoicesMapAndInvalidLowBehaviorIsRejected()
    {
        var sections = SettingsSkeleton.CreateSections();
        Card(sections, "correction.high_confidence_behavior").SelectedValue = "do_nothing";
        Card(sections, "correction.medium_confidence_behavior").SelectedValue = "silent";
        var config = ConfigFormMapper.BuildConfig(sections);
        Assert.AreEqual("do_nothing", config.Correction.HighConfidenceBehavior);
        Assert.AreEqual("silent", config.Correction.MediumConfidenceBehavior);
        Assert.AreEqual("do_nothing", config.Correction.LowConfidenceBehavior);

        Card(sections, "correction.low_confidence_behavior").SelectedValue = "silent";
        var error = Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));
        Assert.AreEqual("correction.low_confidence_behavior: must be do_nothing", error.Message);
    }

    /// <summary>Edited settings cards map to the corresponding typed config values.</summary>
    [TestMethod]
    public void BuildConfigMapsEditedCards()
    {
        var sections = SettingsSkeleton.CreateSections();
        Card(sections, "general.run_mode").SelectedValue = "allowlist";
        Card(sections, "shortcuts.correct").Hotkey = "Ctrl+Shift+Space";
        Card(sections, "shortcuts.correct_arbitrary_selection").IsEnabled = true;
        Card(sections, "triggers.characters").TextValue = "., ?, !";
        Card(sections, "api.timeout_auto_ms").TextValue = "900";
        Card(sections, "correction.preferred_language").TextValue = "en-US";
        Card(sections, "correction.app_language_overrides").TextValue = "notepad.exe=fr-FR, chrome.exe=de-DE";
        Card(sections, "correction.uncertain_language_policy").SelectedValue = "do_nothing";
        Card(sections, "correction.mixed_language_policy").SelectedValue = "disable_correction";
        Card(sections, "feedback.show_timeout_notice").IsEnabled = false;

        var config = ConfigFormMapper.BuildConfig(sections);

        Assert.AreEqual("allowlist", config.General.RunMode);
        Assert.AreEqual("Ctrl+Shift+Space", config.Shortcuts.Correct);
        Assert.IsTrue(config.Shortcuts.CorrectArbitrarySelection);
        CollectionAssert.AreEqual(new[] { ".", "?", "!" }, config.Triggers.Characters);
        Assert.AreEqual(900, config.Api.TimeoutAutoMs);
        Assert.AreEqual("en-US", config.Correction.PreferredLanguage);
        CollectionAssert.AreEqual(new[] { "notepad.exe=fr-FR", "chrome.exe=de-DE" }, config.Correction.AppLanguageOverrides);
        Assert.AreEqual("do_nothing", config.Correction.UncertainLanguagePolicy);
        Assert.AreEqual("disable_correction", config.Correction.MixedLanguagePolicy);
        Assert.IsFalse(config.Feedback.ShowTimeoutNotice);
    }

    /// <summary>Grammar switches round-trip and typo-only mode clears all grammar permissions.</summary>
    [TestMethod]
    public void GrammarCategoriesRoundTripAndTyposModeDisablesThem()
    {
        var config = AppConfig.Default();
        config.Correction.Mode = "typos_plus_grammar";
        config.Correction.EnabledGrammarCategories = ["capitalization", "homophones"];
        var sections = SettingsSkeleton.CreateSections(config);

        Assert.IsTrue(Card(sections, "correction.enabled_grammar_categories.capitalization").IsEnabled);
        Assert.IsFalse(Card(sections, "correction.enabled_grammar_categories.spacing").IsEnabled);
        CollectionAssert.AreEqual(config.Correction.EnabledGrammarCategories, ConfigFormMapper.BuildConfig(sections).Correction.EnabledGrammarCategories);

        Card(sections, "correction.enabled_grammar_categories.homophones").IsEnabled = false;
        Card(sections, "correction.enabled_grammar_categories.spacing").IsEnabled = true;
        CollectionAssert.AreEqual(new[] { "capitalization", "spacing" }, ConfigFormMapper.BuildConfig(sections).Correction.EnabledGrammarCategories);

        Card(sections, "correction.mode").SelectedValue = "typos_only";
        Assert.AreEqual(0, ConfigFormMapper.BuildConfig(sections).Correction.EnabledGrammarCategories.Count);
    }

    [TestMethod]
    public void BuildConfigRejectsInvalidValues()
    {
        var sections = SettingsSkeleton.CreateSections();
        Card(sections, "triggers.word_count").TextValue = "0";

        var error = Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));

        Assert.AreEqual("triggers.word_count: must be greater than zero", error.Message);
    }

    [TestMethod]
    public void BuildConfigRejectsInvalidHotkey()
    {
        var sections = SettingsSkeleton.CreateSections();
        Card(sections, "shortcuts.correct").Hotkey = "Space";

        var error = Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));

        Assert.AreEqual("shortcuts.correct: must include a modifier and supported key", error.Message);
    }

    [TestMethod]
    public void BuildConfigRejectsConflictingHotkeys()
    {
        var sections = SettingsSkeleton.CreateSections();
        Card(sections, "shortcuts.correct").Hotkey = "Ctrl+Alt+Space";
        Card(sections, "shortcuts.undo").Hotkey = "Ctrl+Alt+Space";

        var error = Assert.ThrowsException<InvalidDataException>(() => ConfigFormMapper.BuildConfig(sections));

        Assert.AreEqual("shortcuts.undo: must not match correction shortcut", error.Message);
    }

    [TestMethod]
    public void BuildConfigRejectsDuplicatePaths()
    {
        var sections = SettingsSkeleton.CreateSections();
        var duplicate = new SettingCardViewModel { Path = "general.run_mode" };
        sections[0].Settings.Add(duplicate);

        var error = Assert.ThrowsException<InvalidOperationException>(() => ConfigFormMapper.BuildConfig(sections));

        Assert.AreEqual("Duplicate configuration paths found: general.run_mode", error.Message);
    }

    [TestMethod]
    public void BuildConfigRejectsMissingRequiredPath()
    {
        var sections = SettingsSkeleton.CreateSections();
        sections.Single(section => section.Name == "App Rules").Settings.Remove(Card(sections, "general.run_mode"));

        var error = Assert.ThrowsException<InvalidOperationException>(() => ConfigFormMapper.BuildConfig(sections));

        Assert.AreEqual("Missing configuration path: general.run_mode", error.Message);
    }

    private static SettingCardViewModel Card(
        IEnumerable<SettingsSectionViewModel> sections,
        string path) =>
        sections.SelectMany(section => section.Settings).Single(setting => setting.Path == path);
}
