using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class AppRuleStorageTests
{
    [TestMethod]
    public void UpsertListAndDeleteRoundTrip()
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        var rule = new AppRuleItem
        {
            ProcessName = "word.exe",
            WindowTitlePattern = "*admin*",
            ListBehavior = "blocklist",
            ManualShortcutAllowed = false,
            WordCountTriggerAllowed = false,
            CharacterTriggerAllowed = false,
            LocalEngineAllowed = false,
            ApiEngineAllowed = false,
        };

        storage.Upsert(rule);
        var listed = storage.List();

        Assert.IsTrue(listed.Any(item => item.ProcessName == "word.exe" && item.WindowTitlePattern == "*admin*"));
        Assert.IsTrue(storage.Delete("word.exe", "*admin*"));
    }

    [TestMethod]
    public void ResetDefaultsSeedsEditableSafetyRules()
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));

        var rules = storage.ResetDefaults();

        Assert.IsTrue(rules.Any(rule => rule.ProcessName == "cmd.exe" && !rule.ManualShortcutAllowed));
        Assert.IsTrue(rules.Any(rule => rule.ProcessName == "code.exe" && !rule.WordCountTriggerAllowed));
        Assert.IsTrue(rules.Any(rule => rule.ProcessName == "Bitwarden.exe" && rule.ListBehavior == "blocklist"));
    }

    [TestMethod]
    public void DtoMappingUsesSnakeCaseContractFields()
    {
        var rule = new AppRuleItem
        {
            ProcessName = "code.exe",
            WindowTitlePattern = "*repo*",
            ListBehavior = "allowlist",
            ManualShortcutAllowed = true,
            WordCountTriggerAllowed = false,
            CharacterTriggerAllowed = false,
            LocalEngineAllowed = true,
            ApiEngineAllowed = false,
        };

        var dto = AppRuleStorage.ToDto(rule);

        Assert.AreEqual("code.exe", dto.ProcessName);
        Assert.AreEqual("*repo*", dto.WindowTitlePattern);
        Assert.IsFalse(dto.ApiEngineAllowed);
    }
}
