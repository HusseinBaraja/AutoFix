using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;
using Microsoft.Data.Sqlite;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class AppRuleStorageTests
{
    [TestMethod]
    public void FirstOfflineLoadSeedsDefaultsOnlyOnceAndPreservesDeletedRules()
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        var rules = storage.List();
        Assert.IsTrue(rules.Any(r => r.SafetyMode == "terminal" && !r.ManualShortcutAllowed));
        Assert.IsTrue(rules.Any(r => r.SafetyMode == "code_editor" && !r.WordCountTriggerAllowed && !r.ProseContextAllowed));
        Assert.IsTrue(rules.Any(r => r.ProcessName == "Bitwarden.exe" && !r.LocalEngineAllowed && !r.ApiEngineAllowed));
        Assert.IsTrue(storage.Delete("code.exe", null));
        Assert.IsFalse(storage.List().Any(r => r.ProcessName == "code.exe"));
        Assert.IsTrue(storage.ResetDefaults().Any(r => r.ProcessName == "code.exe"));
    }

    [DataTestMethod]
    [DataRow("", "auto", false)]
    [DataRow(",\"safety_mode\":null", "auto", false)]
    [DataRow(",\"safety_mode\":\"terminal\"", "terminal", false)]
    [DataRow(",\"safety_mode\":\"code_editor\",\"prose_context_allowed\":true", "code_editor", true)]
    public void IpcSafetyFieldsRoundTripThroughStorage(string safetyFields, string expectedMode, bool expectedProse)
    {
        var json = """
            {"type":"app_rules","payload":{"rules":[{"process_name":"code.exe","list_behavior":"allowlist","manual_shortcut_allowed":true,"word_count_trigger_allowed":true,"character_trigger_allowed":false,"local_engine_allowed":true,"api_engine_allowed":false
            """ + safetyFields + "}]}}";
        var envelope = System.Text.Json.JsonSerializer.Deserialize<IpcEnvelope>(json)!;
        var response = envelope.ReadPayload<AppRulesResponse>("app_rules");
        Assert.IsNull(response.Error);
        var rule = AppRuleStorage.FromDto(response.Value!.Rules.Single());
        AppRuleStorage.Validate(rule);

        using var fixture = TempConfigFixture.Create();
        var storage = new AppRuleStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        storage.Upsert(rule);
        var saved = storage.List().Single(item => item.ProcessName == rule.ProcessName);
        Assert.AreEqual(expectedMode, saved.SafetyMode);
        Assert.AreEqual(expectedProse, saved.ProseContextAllowed);
        Assert.IsTrue(saved.ManualShortcutAllowed);
        Assert.IsTrue(saved.WordCountTriggerAllowed);
        Assert.IsFalse(saved.CharacterTriggerAllowed);
        Assert.IsTrue(saved.LocalEngineAllowed);
        Assert.IsFalse(saved.ApiEngineAllowed);
        Assert.AreEqual(AppRuleStorage.ToDto(rule), AppRuleStorage.ToDto(saved));
    }

    [DataTestMethod]
    [DataRow("")]
    [DataRow("unsafe")]
    public void DtoMappingPreservesInvalidSafetyModesForValidation(string safetyMode)
    {
        var dto = new AppRuleDto("code.exe", null, "allowlist", true, false, false, true, true, safetyMode);
        var rule = AppRuleStorage.FromDto(dto);
        Assert.AreEqual(safetyMode, rule.SafetyMode);
        Assert.ThrowsException<ArgumentException>(() => AppRuleStorage.Validate(rule));
    }

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
            SafetyMode = "code_editor",
            ProseContextAllowed = true,
        };

        storage.Upsert(rule);
        var listed = storage.List();

        Assert.IsTrue(listed.Any(item => item.ProcessName == "word.exe" && item.WindowTitlePattern == "*admin*"));
        Assert.AreEqual("code_editor", listed.Single(item => item.ProcessName == "word.exe").SafetyMode);
        Assert.IsTrue(listed.Single(item => item.ProcessName == "word.exe").ProseContextAllowed);
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
        Assert.IsTrue(rules.Any(rule => rule.ProcessName == "code.exe" && !rule.ManualShortcutAllowed && !rule.ProseContextAllowed && rule.SafetyMode == "code_editor"));
        Assert.IsTrue(rules.Any(rule => rule.ProcessName == "cmd.exe" && !rule.WordCountTriggerAllowed && !rule.CharacterTriggerAllowed && rule.SafetyMode == "terminal"));
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
        Assert.AreEqual("auto", listed[0].SafetyMode);
        Assert.IsFalse(listed[0].ProseContextAllowed);
        Assert.IsFalse(listed[0].ManualShortcutAllowed);
        // Repeat migration and save; existing trigger choices remain intact.
        listed[0].SafetyMode = "code_editor";
        listed[0].ProseContextAllowed = true;
        storage.Upsert(listed[0]);
        Assert.IsTrue(storage.List().Single().ProseContextAllowed);
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
            SafetyMode = "code_editor",
            ProseContextAllowed = true,
        };

        var dto = AppRuleStorage.ToDto(rule);

        Assert.AreEqual("code.exe", dto.ProcessName);
        Assert.AreEqual("*repo*", dto.WindowTitlePattern);
        Assert.IsFalse(dto.ApiEngineAllowed);
        var mapped = AppRuleStorage.FromDto(dto);
        Assert.AreEqual("code_editor", mapped.SafetyMode);
        Assert.IsTrue(mapped.ProseContextAllowed);
        Assert.IsTrue(mapped.Clone().ProseContextAllowed);
        var json = System.Text.Json.JsonSerializer.Serialize(dto);
        StringAssert.Contains(json, "\"safety_mode\":\"code_editor\"");
        StringAssert.Contains(json, "\"prose_context_allowed\":true");
    }

    [TestMethod]
    public void NewRulesAndLegacyDtosKeepSafetyOptInsDisabled()
    {
        var rule = new AppRuleItem();
        Assert.IsFalse(rule.ManualShortcutAllowed);
        Assert.IsFalse(rule.WordCountTriggerAllowed);
        Assert.IsFalse(rule.CharacterTriggerAllowed);
        Assert.IsFalse(rule.ProseContextAllowed);
        var dto = System.Text.Json.JsonSerializer.Deserialize<AutoFix.SettingsUi.Ipc.AppRuleDto>(
            """{"process_name":"code.exe","list_behavior":"allowlist","manual_shortcut_allowed":true,"word_count_trigger_allowed":false,"character_trigger_allowed":false,"local_engine_allowed":true,"api_engine_allowed":true}""")!;
        Assert.AreEqual("auto", dto.SafetyMode);
        Assert.IsFalse(dto.ProseContextAllowed);
        rule.SafetyMode = "unsafe";
        rule.ProcessName = "custom.exe";
        Assert.ThrowsException<ArgumentException>(() => AppRuleStorage.Validate(rule));
    }
}
