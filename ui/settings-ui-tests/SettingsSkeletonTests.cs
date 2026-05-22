using System.Globalization;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class SettingsSkeletonTests
{
    [TestMethod]
    public void CreateSectionsIncludesExpectedSettingsAreas()
    {
        var sections = SettingsSkeleton.CreateSections();
        var names = sections.Select(section => section.Name).ToArray();

        CollectionAssert.AreEqual(
            new[]
            {
                "General",
                "Shortcuts",
                "Triggers",
                "Correction",
                "Engines",
                "Context",
                "Feedback",
                "Logs / Debug",
                "Advanced",
            },
            names);
    }

    [TestMethod]
    public void CreateSectionsIncludesRequestedConfigControls()
    {
        var sections = SettingsSkeleton.CreateSections();

        var paths = sections.SelectMany(section => section.Settings).Select(setting => setting.Path).ToArray();

        CollectionAssert.IsSubsetOf(
            new[]
            {
                "general.start_with_windows",
                "general.run_mode",
                "shortcuts.correct",
                "shortcuts.undo",
                "triggers.word_count_enabled",
                "triggers.word_count",
                "triggers.character_trigger_enabled",
                "triggers.characters",
                "correction.enabled",
                "correction.mode",
                "correction.engine",
                "api.timeout_auto_ms",
                "api.fallback_to_local",
                "logging.debug_mode_enabled",
            },
            paths);
    }

    [TestMethod]
    public void AppRulesChangesNotifyHasAppRules()
    {
        var section = new SettingsSectionViewModel();
        var changedProperties = new List<string?>();
        section.PropertyChanged += (_, args) => changedProperties.Add(args.PropertyName);

        section.AppRules.Add(new AppRuleItem { App = "Notepad", Scope = "Blocked", Notes = "Test rule" });
        section.AppRules.Clear();

        CollectionAssert.AreEqual(
            new[] { nameof(SettingsSectionViewModel.HasAppRules), nameof(SettingsSectionViewModel.HasAppRules) },
            changedProperties);
    }

    [TestMethod]
    public void DictionaryChangesNotifyHasDictionary()
    {
        var section = new SettingsSectionViewModel();
        var changedProperties = new List<string?>();
        section.PropertyChanged += (_, args) => changedProperties.Add(args.PropertyName);

        section.Dictionary.Add(new DictionaryItem { Word = "teh", Language = "English", Source = "Custom" });
        section.Dictionary.Clear();

        CollectionAssert.AreEqual(
            new[] { nameof(SettingsSectionViewModel.HasDictionary), nameof(SettingsSectionViewModel.HasDictionary) },
            changedProperties);
    }

    [TestMethod]
    public void CreateSectionsProvidesDropdownOptions()
    {
        var sections = SettingsSkeleton.CreateSections();

        var correction = sections
            .Single(section => section.Name == "Correction")
            .Settings
            .Single(setting => setting.Title == "Correction mode");

        CollectionAssert.AreEqual(
            new[] { "typos_only", "typos_plus_grammar" },
            correction.Options.Select(option => option.Value).ToArray());
    }

    [TestMethod]
    public void DropdownOptionsRenderAsLabels()
    {
        var option = SettingsSkeleton.Modes().First();

        Assert.AreEqual("Typos only", option.ToString());
    }

    [TestMethod]
    public void CreateSectionsFormatsNumericTextValuesWithInvariantCulture()
    {
        var originalCulture = CultureInfo.CurrentCulture;

        try
        {
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("fr-FR");
            var config = AppConfig.Default();
            config.Api.Temperature = 1.25;
            config.Logging.LogRetentionDays = 30;

            var values = SettingsSkeleton.CreateSections(config)
                .SelectMany(section => section.Settings)
                .Where(setting => setting.Kind == "Text")
                .ToDictionary(setting => setting.Path, setting => setting.TextValue);

            Assert.AreEqual("10", values["triggers.word_count"]);
            Assert.AreEqual("3000", values["api.timeout_manual_ms"]);
            Assert.AreEqual("700", values["api.timeout_auto_ms"]);
            Assert.AreEqual("1", values["api.retry_count"]);
            Assert.AreEqual("1.25", values["api.temperature"]);
            Assert.AreEqual("25", values["context.initial_context_words"]);
            Assert.AreEqual("5", values["context.forward_movement_word_limit"]);
            Assert.AreEqual("2000", values["context.informative_context_max_chars"]);
            Assert.AreEqual("25", values["context.informative_context_min_words"]);
            Assert.AreEqual("80", values["context.executable_context_max_words"]);
            Assert.AreEqual("30", values["logging.log_retention_days"]);
        }
        finally
        {
            CultureInfo.CurrentCulture = originalCulture;
        }
    }

    [TestMethod]
    public void CreateSectionsPlacesConfigTransferOnAdvanced()
    {
        var sections = SettingsSkeleton.CreateSections();

        var advanced = sections.Single(section => section.Name == "Advanced");

        Assert.IsTrue(advanced.Settings.Single(setting => setting.Title == "Settings import/export").IsConfigTransfer);
        Assert.IsFalse(sections
            .Where(section => section.Name != "Advanced")
            .SelectMany(section => section.Settings)
            .Any(setting => setting.IsConfigTransfer));
    }

    [TestMethod]
    public void CreateSectionsPlacesBackgroundStatusFirstOnGeneralOnly()
    {
        var sections = SettingsSkeleton.CreateSections();

        var general = sections.Single(section => section.Name == "General");

        Assert.IsTrue(general.Settings.First().IsBackgroundStatus);
        Assert.IsFalse(sections
            .Where(section => section.Name != "General")
            .SelectMany(section => section.Settings)
            .Any(setting => setting.IsBackgroundStatus));
    }
}
