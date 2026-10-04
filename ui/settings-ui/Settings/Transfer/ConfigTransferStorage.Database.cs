using System.IO;
using System.Text;
using System.Text.Json;
using AutoFix.SettingsUi.Ipc;
using Microsoft.Data.Sqlite;

namespace AutoFix.SettingsUi.Settings.Transfer;

public sealed partial class ConfigTransferStorage
{
    /// <summary>Applies the reviewed in-memory payload, refusing stale previews and compensating file failures.</summary>
    public void Apply(ConfigImportPreview preview)
        => Apply(preview, transaction => transaction.Commit());

    /// <summary>Keeps transaction completion injectable for deterministic commit and recovery failure checks.</summary>
    internal void Apply(ConfigImportPreview preview, Action<SqliteTransaction> commit, Action<ImportStage>? checkpoint = null)
    {
        var bundle = Validate(preview.Bundle);
        using var access = ConfigFileAccess.Acquire(configStorage.ConfigPath);
        if (!Path.GetFullPath(appRuleStorage.DatabasePath).Equals(ConfigFileAccess.DatabasePath(configStorage.ConfigPath), StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Import recovery requires settings and product data in the same AutoFix directory.");
        ImportRecovery.Recover(configStorage.ConfigPath, appRuleStorage.DatabasePath);
        EnsureProductTables();
        using var connection = OpenDatabase();
        ImportRecovery.EnsureTable(connection);
        var oldSettings = File.ReadAllBytes(configStorage.ConfigPath);
        var importedSettings = Encoding.UTF8.GetBytes(ConfigStorage.ToToml(bundle.Settings));
        using (var preparation = connection.BeginTransaction())
        {
            var current = ReadData(connection, preparation, bundle.Data?.LearnedRules is not null);
            if (Fingerprint(oldSettings, current) != preview.Fingerprint)
                throw new InvalidDataException("Settings or rules changed after preview. Cancel and preview the import again.");
            ImportRecovery.Prepare(connection, preparation, oldSettings, importedSettings);
            preparation.Commit(); // Durable rollback decision precedes every file or policy replacement.
        }
        var temporary = Path.Combine(Path.GetDirectoryName(Path.GetFullPath(configStorage.ConfigPath))!, $".autofix-import-{Guid.NewGuid():N}.tmp");
        var backup = temporary + ".backup";
        Exception? failure = null;
        try
        {
            checkpoint?.Invoke(ImportStage.Prepared);
            ConfigFileAccess.WriteDurable(backup, oldSettings);
            ConfigFileAccess.WriteDurable(temporary, importedSettings);
            using (var transaction = connection.BeginTransaction())
            {
                var current = ReadData(connection, transaction, bundle.Data?.LearnedRules is not null);
                if (Fingerprint(oldSettings, current) != preview.Fingerprint)
                    throw new InvalidDataException("Rules changed while preparing the import. Preview the import again.");
                if (bundle.Data is { } data) ReplaceData(connection, transaction, data);
                ImportRecovery.MarkCommitted(connection, transaction);
                ConfigFileAccess.ReplaceDurable(temporary, configStorage.ConfigPath);
                checkpoint?.Invoke(ImportStage.SettingsReplaced);
                commit(transaction); // Product rows and the forward-recovery decision commit together.
            }
            checkpoint?.Invoke(ImportStage.Committed);
            ImportRecovery.Recover(configStorage.ConfigPath, appRuleStorage.DatabasePath);
        }
        catch (Exception error)
        {
            failure = error;
            try { ImportRecovery.Recover(configStorage.ConfigPath, appRuleStorage.DatabasePath); }
            catch (Exception restoreError) when (restoreError is IOException or UnauthorizedAccessException or System.Security.SecurityException or SqliteException)
            {
                error.Data["SettingsRestoreError"] = restoreError;
                if (File.Exists(backup)) error.Data["SettingsBackupPath"] = backup;
                error.Data["SettingsRecoveryDatabase"] = appRuleStorage.DatabasePath;
            }
            throw;
        }
        finally
        {
            DeleteImportFile(temporary, failure);
            if (failure?.Data.Contains("SettingsBackupPath") != true) DeleteImportFile(backup, failure);
        }
    }

    /// <summary>Preserves the primary failure if cleanup also fails; successful imports still report cleanup errors.</summary>
    private static void DeleteImportFile(string path, Exception? failure)
    {
        try { if (File.Exists(path)) File.Delete(path); }
        catch (Exception error) when (failure is not null && (error is IOException or UnauthorizedAccessException or System.Security.SecurityException))
        {
            failure.Data["ImportCleanupError"] = error;
            failure.Data["ImportCleanupPath"] = path;
        }
    }

    /// <summary>Reports the primary import error together with retained recovery files and secondary failures.</summary>
    internal static string DescribeFailure(Exception error)
    {
        var message = error.Message;
        if (error.Data["SettingsRestoreError"] is Exception restoreError)
        {
            message += $" | Import recovery is pending: {restoreError.Message}.";
            if (error.Data["SettingsBackupPath"] is string backup) message += $" Original settings backup: {backup}.";
            message += $" Durable recovery database: {error.Data["SettingsRecoveryDatabase"]}. AutoFix blocks correction until recovery succeeds.";
        }
        if (error.Data["ImportCleanupError"] is Exception cleanupError)
            message += $" | Import file could not be removed: {cleanupError.Message}. File: {error.Data["ImportCleanupPath"]}";
        return message;
    }

    private void EnsureProductTables()
    {
        appRuleStorage.List();
        new DictionaryStorage(appRuleStorage.DatabasePath).List();
        // The engine owns schema versions and creation of the language_overrides table.
    }

    private SqliteConnection OpenDatabase()
    {
        var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = appRuleStorage.DatabasePath, DefaultTimeout = 2 }.ToString());
        try { connection.Open(); return connection; }
        catch { connection.Dispose(); throw; }
    }

    private static TransferData ReadData(SqliteConnection connection, SqliteTransaction transaction, bool includeLearned)
    {
        var apps = Read(connection, transaction, """
            select process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed, api_engine_allowed,
                safety_mode, prose_context_allowed from app_rules order by process_name, window_title_pattern
            """, reader => new AppRuleDto(reader.GetString(0), NullableString(reader, 1), reader.GetString(2),
                reader.GetBoolean(3), reader.GetBoolean(4), reader.GetBoolean(5), reader.GetBoolean(6), reader.GetBoolean(7),
                reader.GetString(8), reader.GetBoolean(9)));
        var dictionary = Read(connection, transaction, """
            select entry, language_code, app_process_name from custom_dictionary_entries
            order by entry, language_code, app_process_name
            """, reader => new DictionaryEntry(reader.GetString(0), reader.GetString(1), NullableString(reader, 2)));
        var learned = includeLearned ? Read(connection, transaction, """
            select learning_enabled, original_text, rejected_correction, rule_type, language_code, app_process_name
            from learned_correction_rules order by original_text, rejected_correction, rule_type, language_code, app_process_name, learning_enabled
            """, reader => new LearnedRule(reader.GetBoolean(0), reader.GetString(1), NullableString(reader, 2), reader.GetString(3),
                NullableString(reader, 4), NullableString(reader, 5))) : null;
        var languages = HasLanguageTable(connection, transaction) ? Read(connection, transaction,
            "select app_process_name, language_code from language_overrides order by app_process_name",
            reader => new LanguageOverride(reader.GetString(0), reader.GetString(1))) : [];
        return new(apps, dictionary, learned, languages);
    }

    private static List<T> Read<T>(SqliteConnection connection, SqliteTransaction transaction, string sql, Func<SqliteDataReader, T> row)
    {
        using var command = connection.CreateCommand(); command.Transaction = transaction; command.CommandText = sql;
        using var reader = command.ExecuteReader();
        var result = new List<T>();
        while (reader.Read())
        {
            if (result.Count == MaxRows) throw new InvalidDataException("Too many product rules to transfer.");
            result.Add(row(reader));
        }
        return result;
    }
    private static string? NullableString(SqliteDataReader reader, int column) => reader.IsDBNull(column) ? null : reader.GetString(column);
    private static bool HasLanguageTable(SqliteConnection connection, SqliteTransaction transaction)
    {
        using var command = connection.CreateCommand(); command.Transaction = transaction;
        command.CommandText = "select count(*) from sqlite_master where type = 'table' and name = 'language_overrides'";
        return (long)command.ExecuteScalar()! > 0;
    }

    private static void ReplaceData(SqliteConnection connection, SqliteTransaction transaction, TransferData data)
    {
        Execute(connection, transaction, "delete from app_rules");
        foreach (var row in data.AppRules)
            Execute(connection, transaction, """
                insert into app_rules (process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                    word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed, api_engine_allowed,
                    safety_mode, prose_context_allowed) values ($p0, $p1, $p2, $p3, $p4, $p5, $p6, $p7, $p8, $p9)
                """, row.ProcessName, row.WindowTitlePattern ?? "", row.ListBehavior, row.ManualShortcutAllowed,
                row.WordCountTriggerAllowed, row.CharacterTriggerAllowed, row.LocalEngineAllowed, row.ApiEngineAllowed,
                row.SafetyMode ?? "auto", row.ProseContextAllowed);
        Execute(connection, transaction, "delete from custom_dictionary_entries");
        foreach (var row in data.Dictionary)
            Execute(connection, transaction, "insert into custom_dictionary_entries (entry, language_code, app_process_name) values ($p0, $p1, $p2)",
                row.Word, row.Language, row.App);
        if (data.LearnedRules is { } learned)
        {
            Execute(connection, transaction, "delete from learned_correction_rules");
            foreach (var row in learned)
                Execute(connection, transaction, """
                    insert into learned_correction_rules (learning_enabled, original_text, rejected_correction, rule_type,
                        language_code, app_process_name) values ($p0, $p1, $p2, $p3, $p4, $p5)
                    """, row.Enabled, row.Original, row.Replacement, row.RuleType, row.Language, row.App);
        }
        if (HasLanguageTable(connection, transaction))
        {
            Execute(connection, transaction, "delete from language_overrides");
            foreach (var row in data.LanguageOverrides)
                Execute(connection, transaction, "insert into language_overrides (app_process_name, language_code) values ($p0, $p1)", row.Process, row.Language);
        }
        // All overrides also live in settings.toml, so fresh databases need no UI-owned migration.
    }
    private static void Execute(SqliteConnection connection, SqliteTransaction transaction, string sql, params object?[] values)
    {
        using var command = connection.CreateCommand(); command.Transaction = transaction; command.CommandText = sql;
        for (var index = 0; index < values.Length; index++) command.Parameters.AddWithValue($"$p{index}", values[index] ?? DBNull.Value);
        command.ExecuteNonQuery();
    }

    private static IReadOnlyList<ImportChange> DescribeChanges(AppConfig current, TransferData currentData, ConfigBundle imported)
    {
        var changes = new List<ImportChange>();
        var before = Flatten(JsonSerializer.SerializeToElement(current));
        var after = Flatten(JsonSerializer.SerializeToElement(imported.Settings));
        foreach (var pair in after)
            if (before.GetValueOrDefault(pair.Key) != pair.Value)
                changes.Add(new("Settings", pair.Key, before.GetValueOrDefault(pair.Key, "(none)"), pair.Value));
        if (imported.Data is not { } data) return changes;
        DescribeRows(changes, "App rules", currentData.AppRules, data.AppRules,
            row => row.ProcessName.ToLowerInvariant() + " | " + row.WindowTitlePattern,
            row => $"{row.ListBehavior}; manual={row.ManualShortcutAllowed}, words={row.WordCountTriggerAllowed}, chars={row.CharacterTriggerAllowed}; local={row.LocalEngineAllowed}, API={row.ApiEngineAllowed}; {row.SafetyMode}, prose={row.ProseContextAllowed}");
        DescribeRows(changes, "Dictionary", currentData.Dictionary, data.Dictionary,
            row => $"{row.Word} | {row.Language.ToLowerInvariant()} | {NormalizeApp(row.App) ?? "all apps"}", _ => "Protected");
        DescribeRows(changes, "Languages", currentData.LanguageOverrides, data.LanguageOverrides,
            row => row.Process.ToLowerInvariant(), row => row.Language);
        if (data.LearnedRules is not null)
        {
            var oldRules = currentData.LearnedRules ?? [];
            var oldSet = oldRules.ToHashSet();
            var newSet = data.LearnedRules.ToHashSet();
            var added = data.LearnedRules.Count(row => !oldSet.Contains(row));
            var removed = oldRules.Count(row => !newSet.Contains(row));
            if (added > 0 || removed > 0) changes.Add(new("Learned rules", "Saved exclusions (text hidden in preview)",
                $"{oldRules.Count} rules; {removed} removed", $"{data.LearnedRules.Count} rules; {added} added"));
        }
        return changes;
    }
    private static void DescribeRows<T>(List<ImportChange> changes, string area, IReadOnlyList<T> before, IReadOnlyList<T> after,
        Func<T, string> key, Func<T, string> display)
    {
        var oldRows = before.GroupBy(key).ToDictionary(group => group.Key, group => display(group.Last()));
        var newRows = after.ToDictionary(key, display);
        foreach (var name in oldRows.Keys.Union(newRows.Keys).Order())
        {
            var oldValue = oldRows.GetValueOrDefault(name, "(absent)");
            var newValue = newRows.GetValueOrDefault(name, "(absent)");
            if (oldValue != newValue) changes.Add(new(area, name, oldValue, newValue));
        }
    }
    private static Dictionary<string, string> Flatten(JsonElement root)
    {
        var result = new Dictionary<string, string>();
        void Visit(JsonElement element, string path)
        {
            if (element.ValueKind == JsonValueKind.Object)
                foreach (var property in element.EnumerateObject()) Visit(property.Value, path.Length == 0 ? property.Name : path + "." + property.Name);
            else result[path] = element.ToString();
        }
        Visit(root, ""); return result;
    }
}
