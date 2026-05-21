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
                "App Rules",
                "Languages",
                "Dictionary",
                "Privacy & Security",
                "Feedback",
                "Logs / Debug",
                "Advanced",
            },
            names);
    }

    [TestMethod]
    public void CreateSectionsIncludesTablesForRulesAndDictionary()
    {
        var sections = SettingsSkeleton.CreateSections();

        var appRules = sections.Single(section => section.Name == "App Rules");
        var dictionary = sections.Single(section => section.Name == "Dictionary");

        Assert.IsTrue(appRules.HasAppRules);
        Assert.IsTrue(dictionary.HasDictionary);
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
