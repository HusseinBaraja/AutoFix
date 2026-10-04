using AutoFix.SettingsUi.Settings;
using Microsoft.Data.Sqlite;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class LogStorageTests
{
    [TestMethod]
    public void MissingLogsAreEmptyWithoutCreatingDatabase()
    {
        using var fixture = TempConfigFixture.Create();
        var path = Path.Combine(fixture.Root, "absent.sqlite");
        var storage = new LogStorage(path);
        Assert.AreEqual(0, storage.List().Count);
        storage.Clear();
        Assert.IsFalse(File.Exists(path));
    }

    [TestMethod]
    public void ViewerIsBoundedAndMetadataOnlyAndClearKeepsProductData()
    {
        using var fixture = TempConfigFixture.Create();
        var path = Path.Combine(fixture.Root, "autofix.sqlite");
        var dictionary = new DictionaryStorage(path);
        dictionary.Save(new() { Word = "AutoFix", Language = "und", Source = "dictionary" });
        var rules = new AppRuleStorage(path); rules.List();
        using var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = path }.ToString());
        connection.Open();
        using var command = connection.CreateCommand();
        command.CommandText = """
            create table correction_metadata (id integer primary key, occurred_at text default current_timestamp,
                app_process_name text, trigger_type text, confidence_tier text, engine_used text,
                replacement_method text, result_reason text, latency_ms integer);
            create table debug_events (id integer primary key, message text, typed_text text);
            insert into debug_events (message, typed_text) values ('private-debug-message', 'private-typed-text');
            with recursive n(x) as (select 1 union all select x + 1 from n where x < 505)
            insert into correction_metadata (app_process_name, trigger_type, confidence_tier, engine_used,
                replacement_method, result_reason, latency_ms)
            select 'notepad.exe', 'manual', 'high', 'local', 'clipboard', 'applied', x from n;
            """;
        command.ExecuteNonQuery();
        var storage = new LogStorage(path);
        var rows = storage.List();
        Assert.AreEqual(500, rows.Count);
        Assert.AreEqual(505L, rows[0].LatencyMs);
        Assert.AreEqual(6L, rows[^1].LatencyMs);
        var json = System.Text.Json.JsonSerializer.Serialize(rows);
        Assert.IsFalse(json.Contains("private-typed-text") || json.Contains("private-debug-message"));
        storage.Clear();
        Assert.AreEqual(0, storage.List().Count);
        command.CommandText = "select count(*) from debug_events";
        Assert.AreEqual(0L, command.ExecuteScalar());
        Assert.AreEqual(1, dictionary.List().Count);
        Assert.IsTrue(rules.List().Any(r => r.ProcessName == "Bitwarden.exe"));
    }

    [TestMethod]
    public void ViewerReadsLegacyMetadataWithoutMigratingDatabase()
    {
        using var fixture = TempConfigFixture.Create();
        var path = Path.Combine(fixture.Root, "legacy.sqlite");
        using var connection = new SqliteConnection($"Data Source={path}");
        connection.Open();
        using var command = connection.CreateCommand();
        command.CommandText = """
            create table correction_metadata (id integer primary key, occurred_at text,
                trigger_type text, confidence_tier text, engine_used text,
                replacement_method text, result_reason text, latency_ms integer);
            insert into correction_metadata values (1, '2026-10-04 00:00:00', 'manual', 'high', 'local', 'clipboard', 'applied', 20);
            """;
        command.ExecuteNonQuery();
        Assert.AreEqual("unknown", new LogStorage(path).List().Single().App);
        command.CommandText = "select count(*) from pragma_table_info('correction_metadata') where name = 'app_process_name'";
        Assert.AreEqual(0L, command.ExecuteScalar());
    }

    [TestMethod]
    public void ClearingLogsRollsBackBothTablesOnFailure()
    {
        using var fixture = TempConfigFixture.Create();
        var path = Path.Combine(fixture.Root, "autofix.sqlite");
        using var connection = new SqliteConnection($"Data Source={path}");
        connection.Open();
        using var command = connection.CreateCommand();
        command.CommandText = """
            create table correction_metadata (id integer primary key);
            create table debug_events (id integer primary key);
            insert into correction_metadata values (1);
            insert into debug_events values (1);
            create trigger fail_clear before delete on debug_events begin select raise(abort, 'busy'); end;
            """;
        command.ExecuteNonQuery();
        Assert.ThrowsException<SqliteException>(() => new LogStorage(path).Clear());
        command.CommandText = "select count(*) from correction_metadata";
        Assert.AreEqual(1L, command.ExecuteScalar());
    }
}
