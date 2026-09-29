using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ConfigFormMapperTests
{
    [TestMethod]
    public void BuildConfigMapsEditedCards()
    {
        var sections = SettingsSkeleton.CreateSections();
        Card(sections, "general.run_mode").SelectedValue = "allowlist";
        Card(sections, "shortcuts.correct").Hotkey = "Ctrl+Shift+Space";
        Card(sections, "shortcuts.correct_arbitrary_selection").IsEnabled = true;
        Card(sections, "triggers.characters").TextValue = "., ?, !";
        Card(sections, "api.timeout_auto_ms").TextValue = "900";
        Card(sections, "feedback.show_timeout_notice").IsEnabled = false;

        var config = ConfigFormMapper.BuildConfig(sections);

        Assert.AreEqual("allowlist", config.General.RunMode);
        Assert.AreEqual("Ctrl+Shift+Space", config.Shortcuts.Correct);
        Assert.IsTrue(config.Shortcuts.CorrectArbitrarySelection);
        CollectionAssert.AreEqual(new[] { ".", "?", "!" }, config.Triggers.Characters);
        Assert.AreEqual(900, config.Api.TimeoutAutoMs);
        Assert.IsFalse(config.Feedback.ShowTimeoutNotice);
    }

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
        sections[0].Settings.Remove(Card(sections, "general.run_mode"));

        var error = Assert.ThrowsException<InvalidOperationException>(() => ConfigFormMapper.BuildConfig(sections));

        Assert.AreEqual("Missing configuration path: general.run_mode", error.Message);
    }

    private static SettingCardViewModel Card(
        IEnumerable<SettingsSectionViewModel> sections,
        string path) =>
        sections.SelectMany(section => section.Settings).Single(setting => setting.Path == path);
}
