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
}
