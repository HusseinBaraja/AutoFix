using System.IO;
using AutoFix.SettingsUi.Models;
using Microsoft.Data.Sqlite;

namespace AutoFix.SettingsUi.Settings;

/// <summary>Reads only correction metadata. Debug text is never selected into the settings viewer.</summary>
public sealed class LogStorage(string databasePath)
{
    /// <summary>Lists the newest metadata, treating absent legacy values as empty strings or zero latency.</summary>
    public IReadOnlyList<MetadataLogItem> List()
    {
        if (!File.Exists(databasePath)) return [];
        using var connection = Open(SqliteOpenMode.ReadOnly);
        if (!HasTable(connection, "correction_metadata")) return [];
        var appColumn = HasAppColumn(connection) ? "app_process_name" : "'unknown'";
        using var command = connection.CreateCommand();
        command.CommandText = $"""
            select occurred_at, {appColumn}, trigger_type, confidence_tier, engine_used,
                   replacement_method, result_reason, latency_ms
            from correction_metadata order by id desc limit 500
            """;
        using var reader = command.ExecuteReader();
        var rows = new List<MetadataLogItem>();
        string ReadString(int column) => reader.IsDBNull(column) ? "" : reader.GetString(column);
        while (reader.Read()) rows.Add(new(ReadString(0), ReadString(1), ReadString(2),
            ReadString(3), ReadString(4), ReadString(5), ReadString(6), reader.IsDBNull(7) ? 0 : reader.GetInt64(7)));
        return rows;
    }

    /// <summary>Clears both log tables atomically while preserving dictionary and policy data.</summary>
    public void Clear()
    {
        if (!File.Exists(databasePath)) return;
        using var connection = Open(SqliteOpenMode.ReadWrite);
        using (var secureDelete = connection.CreateCommand())
        {
            secureDelete.CommandText = "pragma secure_delete = on";
            secureDelete.ExecuteNonQuery();
        }
        using var transaction = connection.BeginTransaction();
        foreach (var table in new[] { "correction_metadata", "debug_events" })
        {
            if (!HasTable(connection, table)) continue;
            using var command = connection.CreateCommand();
            command.Transaction = transaction;
            command.CommandText = $"delete from {table}";
            command.ExecuteNonQuery();
        }
        transaction.Commit();
    }

    private SqliteConnection Open(SqliteOpenMode mode)
    {
        var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = databasePath,
            Mode = mode, DefaultTimeout = 2 }.ToString());
        try { connection.Open(); return connection; }
        catch { connection.Dispose(); throw; }
    }

    private static bool HasTable(SqliteConnection connection, string table)
    {
        using var command = connection.CreateCommand();
        command.CommandText = "select count(*) from sqlite_master where type = 'table' and name = $name";
        command.Parameters.AddWithValue("$name", table);
        return (long)command.ExecuteScalar()! != 0;
    }

    private static bool HasAppColumn(SqliteConnection connection)
    {
        using var command = connection.CreateCommand();
        command.CommandText = "pragma table_info(correction_metadata)";
        using var reader = command.ExecuteReader();
        while (reader.Read()) if (reader.GetString(1) == "app_process_name") return true;
        return false;
    }
}
