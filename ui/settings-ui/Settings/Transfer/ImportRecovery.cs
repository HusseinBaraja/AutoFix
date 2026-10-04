using System.IO;
using Microsoft.Data.Sqlite;

namespace AutoFix.SettingsUi.Settings.Transfer;

internal enum ImportStage { Prepared, SettingsReplaced, Committed, RecoveryFileReplaced }

/// <summary>SQLite owns the import decision; the settings file is reconciled before policy-dependent work.</summary>
internal static class ImportRecovery
{
    internal static void EnsureTable(SqliteConnection connection)
    {
        using var command = connection.CreateCommand();
        command.CommandText = """
            pragma synchronous = FULL;
            create table if not exists settings_import_recovery (
                id integer primary key check (id = 1),
                original_settings blob not null, imported_settings blob not null,
                committed integer not null check (committed in (0, 1)));
            """;
        command.ExecuteNonQuery();
    }

    internal static void Prepare(SqliteConnection connection, SqliteTransaction transaction, byte[] original, byte[] imported)
    {
        using var command = connection.CreateCommand(); command.Transaction = transaction;
        command.CommandText = "insert into settings_import_recovery values (1, $original, $imported, 0)";
        command.Parameters.AddWithValue("$original", original);
        command.Parameters.AddWithValue("$imported", imported);
        command.ExecuteNonQuery();
    }

    internal static void MarkCommitted(SqliteConnection connection, SqliteTransaction transaction)
    {
        using var command = connection.CreateCommand(); command.Transaction = transaction;
        command.CommandText = "update settings_import_recovery set committed = 1 where id = 1";
        if (command.ExecuteNonQuery() != 1) throw new InvalidDataException("The durable import record is missing.");
    }

    /// <summary>Must run under the settings-file lock; leaves the record intact if any reconciliation step fails.</summary>
    internal static bool Recover(string configPath, string? databasePath = null, Action? afterFileReplaced = null)
    {
        try { return RecoverCore(configPath, databasePath, afterFileReplaced); }
        catch (SqliteException error) { throw new IOException("Unable to reconcile the durable import record. AutoFix cannot use this configuration yet.", error); }
    }

    private static bool RecoverCore(string configPath, string? databasePath, Action? afterFileReplaced)
    {
        databasePath ??= ConfigFileAccess.DatabasePath(configPath);
        if (!File.Exists(databasePath)) return false;
        using var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = databasePath,
            Mode = SqliteOpenMode.ReadWrite, DefaultTimeout = 2, Pooling = false }.ToString());
        connection.Open();
        using var table = connection.CreateCommand();
        table.CommandText = "select count(*) from sqlite_master where type = 'table' and name = 'settings_import_recovery'";
        if ((long)table.ExecuteScalar()! == 0) return false;
        table.CommandText = "select count(*) from settings_import_recovery";
        if ((long)table.ExecuteScalar()! == 0) return false;
        using var synchronous = connection.CreateCommand();
        synchronous.CommandText = "pragma synchronous = FULL"; synchronous.ExecuteNonQuery();
        using var transaction = connection.BeginTransaction();
        using var command = connection.CreateCommand(); command.Transaction = transaction;
        command.CommandText = "select original_settings, imported_settings, committed from settings_import_recovery where id = 1";
        byte[] bytes;
        using (var reader = command.ExecuteReader())
        {
            if (!reader.Read()) return false;
            bytes = (byte[])reader.GetValue(reader.GetInt64(2) == 1 ? 1 : 0);
        }
        var temporary = configPath + ".recovery.tmp";
        ConfigFileAccess.WriteDurable(temporary, bytes);
        ConfigFileAccess.ReplaceDurable(temporary, configPath);
        afterFileReplaced?.Invoke();
        command.CommandText = "delete from settings_import_recovery where id = 1";
        command.ExecuteNonQuery(); transaction.Commit();
        return true;
    }
}
