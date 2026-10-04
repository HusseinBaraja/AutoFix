using System.IO.Compression;
using System.Text.Json;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.Settings.Transfer;
using Microsoft.Data.Sqlite;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ConfigTransferStorageTests
{
    /// <summary>A missing live config gets default settings and a matching preview fingerprint.</summary>
    [TestMethod]
    public void PreviewCreatesMissingSettingsAndHashesTheDisplayedSnapshot()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var path = Path.Combine(fixture.Root, "bundle.zip");
        Execute(fixture, "delete from app_rules");
        transfer.Export(path, AppConfig.Default(), true);
        File.Delete(fixture.Path);
        var preview = transfer.Preview(path);
        Assert.IsTrue(fixture.Storage.LastLoadCreatedConfig);
        Assert.IsFalse(preview.HasChanges);
        Assert.AreEqual(ConfigTransferStorage.Fingerprint(File.ReadAllBytes(fixture.Path), preview.Bundle.Data!), preview.Fingerprint);
        transfer.Apply(preview);
    }

    /// <summary>Commit failures restore exact settings bytes and roll back all product rows.</summary>
    [TestMethod]
    public void CommitFailureRestoresOriginalSettingsAndPreservesThePrimaryError()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var config = AppConfig.Default(); config.Triggers.WordCount = 22;
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, config, true);
        new DictionaryStorage(Database(fixture)).Save(new() { Word = "keep-word", Language = "und", Source = "dictionary" });
        var preview = transfer.Preview(path);
        var before = File.ReadAllBytes(fixture.Path);
        var primary = new SqliteException("commit failed", 19);
        var error = Assert.ThrowsException<SqliteException>(() => transfer.Apply(preview, _ => throw primary));
        Assert.AreSame(primary, error);
        CollectionAssert.AreEqual(before, File.ReadAllBytes(fixture.Path));
        Assert.AreEqual("keep-word", Scalar(fixture, "select entry from custom_dictionary_entries"));
        Assert.AreEqual(0, Directory.GetFiles(fixture.Root, ".autofix-import-*").Length);
    }

    /// <summary>Failed restoration retains an exact backup and reports it without replacing the commit exception.</summary>
    [TestMethod]
    public void RestoreFailureKeepsBackupAndReportsBothErrors()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var config = AppConfig.Default(); config.Triggers.WordCount = 22;
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, config, true);
        new DictionaryStorage(Database(fixture)).Save(new() { Word = "keep-word", Language = "und", Source = "dictionary" });
        var preview = transfer.Preview(path);
        var before = File.ReadAllBytes(fixture.Path);
        var primary = new SqliteException("commit failed", 19);
        FileStream? locked = null;
        try
        {
            var error = Assert.ThrowsException<SqliteException>(() => transfer.Apply(preview, _ =>
            {
                locked = new FileStream(fixture.Path, FileMode.Open, FileAccess.Read, FileShare.Read);
                throw primary;
            }));
            Assert.AreSame(primary, error);
            Assert.IsInstanceOfType(error.Data["SettingsRestoreError"], typeof(UnauthorizedAccessException));
            var backup = (string)error.Data["SettingsBackupPath"]!;
            CollectionAssert.AreEqual(before, File.ReadAllBytes(backup));
            Assert.AreEqual(22, fixture.Storage.Load(fixture.Path).Triggers.WordCount);
            Assert.AreEqual("keep-word", Scalar(fixture, "select entry from custom_dictionary_entries"));
            StringAssert.Contains(ConfigTransferStorage.DescribeFailure(error), "commit failed");
            StringAssert.Contains(ConfigTransferStorage.DescribeFailure(error), backup);
            Assert.AreEqual(1, Directory.GetFiles(fixture.Root, ".autofix-import-*").Length);
        }
        finally { locked?.Dispose(); }
    }

    /// <summary>A locked temporary file cannot mask a commit failure or stop settings restoration.</summary>
    [TestMethod]
    public void CleanupFailurePreservesCommitErrorAndRestoredSettings()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, AppConfig.Default(), false);
        var preview = transfer.Preview(path);
        var before = File.ReadAllBytes(fixture.Path);
        var primary = new SqliteException("commit failed", 19);
        FileStream? locked = null;
        try
        {
            var error = Assert.ThrowsException<SqliteException>(() => transfer.Apply(preview, _ =>
            {
                var backup = Directory.GetFiles(fixture.Root, ".autofix-import-*.backup").Single();
                var temporary = backup[..^".backup".Length];
                locked = new FileStream(temporary, FileMode.Create, FileAccess.Write, FileShare.None);
                throw primary;
            }));
            Assert.AreSame(primary, error);
            CollectionAssert.AreEqual(before, File.ReadAllBytes(fixture.Path));
            Assert.IsInstanceOfType(error.Data["ImportCleanupError"], typeof(IOException));
            StringAssert.Contains(ConfigTransferStorage.DescribeFailure(error), (string)error.Data["ImportCleanupPath"]!);
            Assert.AreEqual(0, Directory.GetFiles(fixture.Root, "*.backup").Length);
        }
        finally { locked?.Dispose(); }
    }

    [TestMethod]
    public void BundleRoundTripsPermissionsPhrasesScopesAndEffectiveLanguages()
    {
        using var source = TempConfigFixture.Create();
        var transfer = CreateTransfer(source);
        var config = source.Storage.Load(source.Path);
        config.General.RunMode = "allowlist";
        config.Triggers.WordCount = 18;
        config.Correction.PreferredLanguage = "fr";
        config.Correction.AppLanguageOverrides = ["notes.exe=en-US"];
        var apps = new AppRuleStorage(Database(source));
        apps.Upsert(new() { ProcessName = "notes.exe", WindowTitlePattern = "Report", ListBehavior = "allowlist",
            ManualShortcutAllowed = true, WordCountTriggerAllowed = false, CharacterTriggerAllowed = true,
            LocalEngineAllowed = true, ApiEngineAllowed = false, SafetyMode = "code_editor", ProseContextAllowed = true });
        var words = new DictionaryStorage(Database(source));
        words.Save(new() { Word = "AutoFix", Language = "und", Source = "dictionary" });
        words.Save(new() { Word = "projet secret", Language = "fr", App = "notes.exe", Source = "dictionary" });
        words.Save(new() { Word = "私の製品", Language = "ja", Source = "dictionary" });
        words.Save(new() { Word = "learned-source", Replacement = "learned-replacement", Language = "en", App = "notes.exe", Source = "pair" });
        Execute(source, """
            create table language_overrides (id integer primary key, app_process_name text not null unique, language_code text not null, created_at text default current_timestamp);
            insert into language_overrides (app_process_name, language_code) values ('notes.exe', 'de'), ('word.exe', 'ar');
            insert into learned_correction_rules (original_text, rejected_correction, rule_type, learning_enabled) values ('disabled-rule', 'bad-replacement', 'pair', 0);
            """);
        var path = Path.Combine(source.Root, "bundle.zip");
        transfer.Export(path, config, true);

        using var target = TempConfigFixture.Create();
        var targetTransfer = CreateTransfer(target);
        new DictionaryStorage(Database(target)).Save(new() { Word = "old-word", Language = "und", Source = "dictionary" });
        Execute(target, """
            create table language_overrides (id integer primary key, app_process_name text not null unique, language_code text not null);
            insert into language_overrides (app_process_name, language_code) values ('old.exe', 'es');
            """);
        var before = File.ReadAllText(target.Path);
        var preview = targetTransfer.Preview(path);
        Assert.AreEqual(before, File.ReadAllText(target.Path));
        Assert.IsTrue(preview.Changes.Any(c => c.Name == "triggers.word_count" && c.After == "18"));
        Assert.IsTrue(preview.Changes.Any(c => c.Area == "Dictionary" && c.Name.Contains("old-word") && c.After == "(absent)"));
        Assert.IsFalse(JsonSerializer.Serialize(preview.Changes).Contains("learned-source"));
        targetTransfer.Apply(preview);

        var imported = target.Storage.Load(target.Path);
        Assert.AreEqual("allowlist", imported.General.RunMode);
        Assert.AreEqual(18, imported.Triggers.WordCount);
        Assert.AreEqual("fr", imported.Correction.PreferredLanguage);
        CollectionAssert.AreEqual(new[] { "notes.exe=en-US", "word.exe=ar" }, imported.Correction.AppLanguageOverrides.ToArray());
        var rule = new AppRuleStorage(Database(target)).List().Single(r => r.ProcessName == "notes.exe");
        Assert.AreEqual("Report", rule.WindowTitlePattern);
        Assert.IsTrue(rule.ManualShortcutAllowed && rule.CharacterTriggerAllowed && rule.LocalEngineAllowed && rule.ProseContextAllowed);
        Assert.IsFalse(rule.WordCountTriggerAllowed || rule.ApiEngineAllowed);
        Assert.AreEqual("code_editor", rule.SafetyMode);
        var entries = new DictionaryStorage(Database(target)).List();
        Assert.AreEqual(4, entries.Count);
        Assert.IsTrue(entries.Any(d => d.Word == "projet secret" && d.Language == "fr" && d.App == "notes.exe"));
        Assert.IsTrue(entries.Any(d => d.Word == "私の製品"));
        Assert.IsFalse(entries.Any(d => d.Word == "old-word"));
        Assert.AreEqual(2L, Scalar(target, "select count(*) from learned_correction_rules"));
        Assert.AreEqual("en-US", Scalar(target, "select language_code from language_overrides where app_process_name = 'notes.exe'"));
    }

    [TestMethod]
    public void DefaultExportExcludesSecretsSessionHistoryLogsAndLearnedPairs()
    {
        using var source = TempConfigFixture.Create();
        var transfer = CreateTransfer(source);
        File.AppendAllText(source.Path, "\n[secrets]\napi_key = 'private-api-key'\n");
        new DictionaryStorage(Database(source)).Save(new() { Word = "learned-private-original", Replacement = "learned-private-replacement", Language = "und", Source = "pair" });
        Execute(source, """
            create table debug_events (typed_text text);
            insert into debug_events values ('private-session-text');
            create table correction_history (original_text text, corrected_text text);
            insert into correction_history values ('private-original-history', 'private-corrected-history');
            create table correction_metadata (result_reason text);
            insert into correction_metadata values ('private-log-sentinel');
            """);
        var path = Path.Combine(source.Root, "default.zip");
        // Export accepts typed settings, never raw TOML or database bytes.
        transfer.Export(path, AppConfig.Default(), false);
        var contents = Contents(path);
        CollectionAssert.AreEquivalent(new[] { "manifest.json", "settings.toml", "app-rules.json", "dictionary.json", "language-overrides.json" }, contents.Keys.ToArray());
        var text = string.Join("\n", contents.Values);
        Assert.IsFalse(text.Contains("private-") || text.Contains("learned-private-"));

        using var target = TempConfigFixture.Create();
        var targetTransfer = CreateTransfer(target);
        new DictionaryStorage(Database(target)).Save(new() { Word = "kept-pair", Replacement = "kept-replacement", Language = "und", Source = "pair" });
        Execute(target, """
            create table correction_history (original_text text);
            insert into correction_history values ('kept-history');
            create table debug_events (typed_text text);
            insert into debug_events values ('kept-session');
            create table secure_keys (secret text);
            insert into secure_keys values ('kept-key');
            """);
        targetTransfer.Apply(targetTransfer.Preview(path));
        Assert.AreEqual("kept-pair", Scalar(target, "select original_text from learned_correction_rules"));
        Assert.AreEqual("kept-history", Scalar(target, "select original_text from correction_history"));
        Assert.AreEqual("kept-session", Scalar(target, "select typed_text from debug_events"));
        Assert.AreEqual("kept-key", Scalar(target, "select secret from secure_keys"));
    }

    [TestMethod]
    public void FreshDatabaseImportsLanguageOverridesWithoutOwningEngineMigrations()
    {
        using var source = TempConfigFixture.Create();
        var transfer = CreateTransfer(source);
        var config = AppConfig.Default(); config.Correction.AppLanguageOverrides = ["notes.exe=fr"];
        var path = Path.Combine(source.Root, "bundle.zip"); transfer.Export(path, config, false);
        using var target = TempConfigFixture.Create();
        var targetTransfer = CreateTransfer(target);
        targetTransfer.Apply(targetTransfer.Preview(path));
        CollectionAssert.AreEqual(new[] { "notes.exe=fr" }, target.Storage.Load(target.Path).Correction.AppLanguageOverrides.ToArray());
        Assert.AreEqual(0L, Scalar(target, "select count(*) from sqlite_master where name = 'language_overrides'"));
    }

    [DataTestMethod]
    [DataRow("settings")]
    [DataRow("apps")]
    [DataRow("dictionary")]
    [DataRow("learned")]
    [DataRow("languages")]
    public void StalePreviewIsRejectedBeforeAnyReplacement(string changedArea)
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, AppConfig.Default(), true);
        var preview = transfer.Preview(path);
        switch (changedArea)
        {
            case "settings": var config = AppConfig.Default(); config.Triggers.WordCount = 19; fixture.Storage.Save(config); break;
            case "apps": new AppRuleStorage(Database(fixture)).Upsert(new() { ProcessName = "new.exe" }); break;
            case "dictionary": new DictionaryStorage(Database(fixture)).Save(new() { Word = "new", Language = "und", Source = "dictionary" }); break;
            case "learned": new DictionaryStorage(Database(fixture)).Save(new() { Word = "new", Replacement = "newer", Language = "und", Source = "pair" }); break;
            case "languages": Execute(fixture, "create table language_overrides (app_process_name text, language_code text); insert into language_overrides values ('new.exe', 'fr')"); break;
        }
        var before = File.ReadAllText(fixture.Path);
        var dictionaryBefore = JsonSerializer.Serialize(new DictionaryStorage(Database(fixture)).List());
        Assert.ThrowsException<InvalidDataException>(() => transfer.Apply(preview));
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual(dictionaryBefore, JsonSerializer.Serialize(new DictionaryStorage(Database(fixture)).List()));
    }

    [TestMethod]
    public void ReviewedSnapshotSurvivesSourceMutationAndDoesNotReadLogsOnApply()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var config = AppConfig.Default(); config.Triggers.WordCount = 22;
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, config, false);
        var preview = transfer.Preview(path);
        File.WriteAllText(path, "replaced file after preview");
        Execute(fixture, "create table debug_events (typed_text text); insert into debug_events values ('changed-session')");
        transfer.Apply(preview);
        Assert.AreEqual(22, fixture.Storage.Load(fixture.Path).Triggers.WordCount);
        Assert.AreEqual("changed-session", Scalar(fixture, "select typed_text from debug_events"));
    }

    [TestMethod]
    public void NoChangePreviewPreservesLearnedRulesAddedAfterPreviewWhenExcluded()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, AppConfig.Default(), false);
        var preview = transfer.Preview(path);
        Assert.IsFalse(preview.HasChanges);
        StringAssert.Contains(preview.ChangeSummary, "No changes.");
        new DictionaryStorage(Database(fixture)).Save(new() { Word = "just-learned", Replacement = "replacement", Language = "und", Source = "pair" });
        transfer.Apply(preview);
        Assert.AreEqual("just-learned", Scalar(fixture, "select original_text from learned_correction_rules"));
    }

    [TestMethod]
    public void DatabaseFailureRollsBackConfigAndAllReplacedTables()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var config = AppConfig.Default(); config.Triggers.WordCount = 22;
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, config, true);
        new DictionaryStorage(Database(fixture)).Save(new() { Word = "keep-word", Language = "und", Source = "dictionary" });
        var preview = transfer.Preview(path);
        Execute(fixture, "create trigger deny_dictionary_delete before delete on custom_dictionary_entries begin select raise(abort, 'test failure'); end");
        var before = File.ReadAllText(fixture.Path);
        var appsBefore = JsonSerializer.Serialize(new AppRuleStorage(Database(fixture)).List());
        Assert.ThrowsException<SqliteException>(() => transfer.Apply(preview));
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual(appsBefore, JsonSerializer.Serialize(new AppRuleStorage(Database(fixture)).List()));
        Assert.AreEqual("keep-word", Scalar(fixture, "select entry from custom_dictionary_entries"));
        Assert.AreEqual(0, Directory.GetFiles(fixture.Root, "*.tmp").Length);
    }

    [TestMethod]
    public void SettingsFileFailureRollsBackDatabaseReplacements()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, AppConfig.Default(), false);
        new DictionaryStorage(Database(fixture)).Save(new() { Word = "keep-word", Language = "und", Source = "dictionary" });
        var preview = transfer.Preview(path);
        var before = File.ReadAllText(fixture.Path);
        using (var locked = new FileStream(fixture.Path, FileMode.Open, FileAccess.Read, FileShare.Read))
            Assert.ThrowsException<UnauthorizedAccessException>(() => transfer.Apply(preview));
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual("keep-word", Scalar(fixture, "select entry from custom_dictionary_entries"));
    }

    [DataTestMethod]
    [DataRow("missing")]
    [DataRow("version")]
    [DataRow("manifest")]
    [DataRow("extra")]
    [DataRow("traversal")]
    [DataRow("duplicate")]
    [DataRow("settings")]
    [DataRow("secret")]
    [DataRow("null")]
    [DataRow("duplicate-rules")]
    [DataRow("bad-app")]
    [DataRow("bad-language")]
    [DataRow("language-disagreement")]
    [DataRow("bad-learned")]
    [DataRow("oversized")]
    [DataRow("too-many-rules")]
    public void InvalidBundlesNeverChangeLocalConfigOrRules(string fault)
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var path = Path.Combine(fixture.Root, "bundle.zip"); transfer.Export(path, AppConfig.Default(), true);
        var files = Contents(path);
        switch (fault)
        {
            case "missing": files.Remove("dictionary.json"); break;
            case "version": files["manifest.json"] = "{\"format_version\":2,\"learned_rules_included\":true}"; break;
            case "manifest": files["manifest.json"] = "{\"format_version\":1,\"learned_rules_included\":false}"; break;
            case "extra": files["secrets.json"] = "{\"api_key\":\"private\"}"; break;
            case "traversal": files["../settings.toml"] = "traversal"; break;
            case "settings": var invalid = AppConfig.Default(); invalid.Api.TimeoutManualMs = 0; files["settings.toml"] = ConfigStorage.ToToml(invalid); break;
            case "secret": files["dictionary.json"] = "[{\"word\":\"safe\",\"language\":\"und\",\"api_key\":\"private\"}]"; break;
            case "null": files["dictionary.json"] = "[null]"; break;
            case "duplicate-rules": files["dictionary.json"] = "[{\"word\":\"same\",\"language\":\"en\"},{\"word\":\"same\",\"language\":\"EN\"}]"; break;
            case "bad-app": files["app-rules.json"] = "[{\"process_name\":\"../bad.exe\",\"list_behavior\":\"allowlist\"}]"; break;
            case "bad-language": files["dictionary.json"] = "[{\"word\":\"safe\",\"language\":\"invalid!\"}]"; break;
            case "language-disagreement": files["language-overrides.json"] = "[{\"process\":\"notes.exe\",\"language\":\"fr\"}]"; break;
            case "bad-learned": files["learned-rules.json"] = "[{\"original\":\"word\",\"rule_type\":\"unknown\"}]"; break;
            case "oversized": files["dictionary.json"] = new string(' ', 8 * 1024 * 1024 + 1); break;
            case "too-many-rules": files["dictionary.json"] = JsonSerializer.Serialize(Enumerable.Range(0, 10001).Select(i => new { word = $"term-{i}", language = "und" })); break;
        }
        WriteBundle(path, files, fault == "duplicate");
        var before = File.ReadAllText(fixture.Path);
        var appsBefore = JsonSerializer.Serialize(new AppRuleStorage(Database(fixture)).List());
        try { transfer.Preview(path); Assert.Fail("Invalid bundle was accepted."); }
        catch (Exception error) when (error is InvalidDataException or ArgumentException or JsonException) { }
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual(appsBefore, JsonSerializer.Serialize(new AppRuleStorage(Database(fixture)).List()));
    }

    [TestMethod]
    public void ExportCannotOverwriteLiveSettingsOrDatabase()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        var before = File.ReadAllText(fixture.Path);
        Assert.ThrowsException<InvalidDataException>(() => transfer.Export(fixture.Path, AppConfig.Default(), false));
        Assert.ThrowsException<InvalidDataException>(() => transfer.Export(Database(fixture), AppConfig.Default(), false));
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
    }

    [TestMethod]
    public void LegacyTomlImportDropsUnknownSecretsAndPreservesProductRows()
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = CreateTransfer(fixture);
        new DictionaryStorage(Database(fixture)).Save(new() { Word = "keep-word", Language = "und", Source = "dictionary" });
        var appsBefore = JsonSerializer.Serialize(new AppRuleStorage(Database(fixture)).List());
        var config = AppConfig.Default(); config.Triggers.WordCount = 24;
        var path = Path.Combine(fixture.Root, "legacy.toml");
        File.WriteAllText(path, ConfigStorage.ToToml(config) + "\n[secrets]\napi_key = 'private-secret'\n[session]\ntyped_text = 'private-typing'\n");
        var preview = transfer.Preview(path);
        StringAssert.Contains(preview.Summary, "Settings-only");
        Assert.IsFalse(JsonSerializer.Serialize(preview.Changes).Contains("private-"));
        transfer.Apply(preview);
        Assert.AreEqual(24, fixture.Storage.Load(fixture.Path).Triggers.WordCount);
        Assert.IsFalse(File.ReadAllText(fixture.Path).Contains("private-") || File.ReadAllText(fixture.Path).Contains("[secrets]"));
        Assert.AreEqual("keep-word", Scalar(fixture, "select entry from custom_dictionary_entries"));
        Assert.AreEqual(appsBefore, JsonSerializer.Serialize(new AppRuleStorage(Database(fixture)).List()));
    }

    internal static ConfigTransferStorage CreateTransfer(TempConfigFixture fixture)
    {
        fixture.Storage.LoadOrCreate();
        var apps = new AppRuleStorage(Database(fixture)); apps.List();
        new DictionaryStorage(Database(fixture)).List();
        return new(fixture.Storage, apps);
    }
    internal static string Database(TempConfigFixture fixture) => Path.Combine(fixture.Root, "autofix.sqlite");
    internal static void Execute(TempConfigFixture fixture, string sql)
    {
        using var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = Database(fixture) }.ToString());
        connection.Open(); using var command = connection.CreateCommand(); command.CommandText = sql; command.ExecuteNonQuery();
    }
    internal static object? Scalar(TempConfigFixture fixture, string sql)
    {
        using var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = Database(fixture) }.ToString());
        connection.Open(); using var command = connection.CreateCommand(); command.CommandText = sql; return command.ExecuteScalar();
    }
    internal static Dictionary<string, string> Contents(string path)
    {
        using var archive = ZipFile.OpenRead(path);
        return archive.Entries.ToDictionary(entry => entry.FullName, entry => { using var reader = new StreamReader(entry.Open()); return reader.ReadToEnd(); });
    }
    private static void WriteBundle(string path, Dictionary<string, string> files, bool duplicate)
    {
        using var archive = new ZipArchive(File.Create(path), ZipArchiveMode.Create);
        foreach (var file in files)
        {
            using var writer = new StreamWriter(archive.CreateEntry(file.Key).Open()); writer.Write(file.Value);
        }
        if (duplicate) { using var writer = new StreamWriter(archive.CreateEntry("settings.toml").Open()); writer.Write(files["settings.toml"]); }
    }
}
