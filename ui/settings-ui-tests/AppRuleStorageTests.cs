using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;
using Microsoft.Data.Sqlite;

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
    public void UpsertProcessOnlyRuleUpdatesExistingRow()
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        var rule = new AppRuleItem
        {
            ProcessName = "notepad.exe",
            ListBehavior = "allowlist",
            ManualShortcutAllowed = true,
            WordCountTriggerAllowed = false,
            CharacterTriggerAllowed = false,
            LocalEngineAllowed = true,
            ApiEngineAllowed = true,
        };
        storage.Upsert(rule);

        rule.ListBehavior = "blocklist";
        rule.ManualShortcutAllowed = false;
        rule.LocalEngineAllowed = false;
        rule.ApiEngineAllowed = false;
        storage.Upsert(rule);

        var listed = storage.List().Where(item => item.ProcessName == "notepad.exe").ToList();
        Assert.AreEqual(1, listed.Count);
        Assert.AreEqual("", listed[0].WindowTitlePattern);
        Assert.AreEqual("blocklist", listed[0].ListBehavior);
    }

    [TestMethod]
    public void OpeningStorageNormalizesLegacyNullProcessOnlyRules()
    {
        using var fixture = TempConfigFixture.Create();
        var path = Path.Combine(fixture.Root, "autofix.sqlite");
        using (var connection = new SqliteConnection($"Data Source={path}"))
        {
            connection.Open();
            using var command = connection.CreateCommand();
            command.CommandText =
                """
                create table app_rules (
                    id integer primary key,
                    process_name text not null,
                    window_title_pattern text,
                    list_behavior text not null check (list_behavior in ('allowlist', 'blocklist')),
                    manual_shortcut_allowed integer not null check (manual_shortcut_allowed in (0, 1)),
                    word_count_trigger_allowed integer not null check (word_count_trigger_allowed in (0, 1)),
                    character_trigger_allowed integer not null check (character_trigger_allowed in (0, 1)),
                    local_engine_allowed integer not null check (local_engine_allowed in (0, 1)),
                    api_engine_allowed integer not null check (api_engine_allowed in (0, 1)),
                    created_at text not null default current_timestamp,
                    updated_at text not null default current_timestamp,
                    unique (process_name, window_title_pattern)
                );
                insert into app_rules (
                    process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                    word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                    api_engine_allowed
                ) values
                    ('notepad.exe', null, 'allowlist', 1, 0, 0, 1, 1),
                    ('notepad.exe', null, 'blocklist', 0, 0, 0, 0, 0);
                """;
            command.ExecuteNonQuery();
        }

        var storage = new AppRuleStorage(path);
        var listed = storage.List().Where(item => item.ProcessName == "notepad.exe").ToList();

        Assert.AreEqual(1, listed.Count);
        Assert.AreEqual("", listed[0].WindowTitlePattern);
        Assert.AreEqual("blocklist", listed[0].ListBehavior);
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
