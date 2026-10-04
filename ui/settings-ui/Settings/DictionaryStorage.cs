using AutoFix.SettingsUi.Models;
using Microsoft.Data.Sqlite;
using System.IO;

namespace AutoFix.SettingsUi.Settings;

/// <summary>Edits the engine's SQLite exclusions, including learned pairs.</summary>
public sealed class DictionaryStorage(string databasePath)
{
    /// <summary>Lists word exclusions and active pair rules with their language and app scopes.</summary>
    public IReadOnlyList<DictionaryItem> List()
    {
        using var connection = Open();
        using var command = connection.CreateCommand();
        command.CommandText = """
            select id, entry, language_code, coalesce(app_process_name, ''), '', 'dictionary'
            from custom_dictionary_entries
            union all
            select id, original_text, coalesce(language_code, 'und'), coalesce(app_process_name, ''),
                   rejected_correction, 'pair' from learned_correction_rules
            where learning_enabled = 1 and rule_type = 'pair'
            order by 2, 3, 4
            """;
        using var reader = command.ExecuteReader();
        var entries = new List<DictionaryItem>();
        while (reader.Read())
        {
            entries.Add(new DictionaryItem { Id = reader.GetInt64(0), Word = reader.GetString(1),
                Language = reader.GetString(2), App = reader.GetString(3),
                Replacement = reader.IsDBNull(4) ? "" : reader.GetString(4), Source = reader.GetString(5) });
        }
        return entries;
    }

    /// <summary>Replaces an entry atomically; duplicate scopes remain a single exclusion.</summary>
    public void Save(DictionaryItem entry, DictionaryItem? previous = null)
    {
        Validate(entry);
        using var connection = Open();
        using var transaction = connection.BeginTransaction();
        if (previous is not null) Delete(connection, transaction, previous);
        using var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = entry.Source == "dictionary" ? """
            insert into custom_dictionary_entries (language_code, app_process_name, entry)
            select $language, $app, $word where not exists (
                select 1 from custom_dictionary_entries where lower(language_code) = lower($language)
                and coalesce(lower(app_process_name), '') = coalesce(lower($app), '') and entry = $word)
            """ : """
            insert into learned_correction_rules
                (learning_enabled, original_text, rejected_correction, rule_type, language_code, app_process_name)
            select 1, $word, $replacement, 'pair', $language, $app where not exists (
                select 1 from learned_correction_rules where learning_enabled = 1 and rule_type = 'pair'
                and original_text = $word and rejected_correction = $replacement
                and lower(coalesce(language_code, 'und')) = lower($language)
                and coalesce(lower(app_process_name), '') = coalesce(lower($app), ''))
            """;
        command.Parameters.AddWithValue("$language", entry.Language.Trim());
        command.Parameters.AddWithValue("$app", string.IsNullOrWhiteSpace(entry.App) ? DBNull.Value : entry.App.Trim().ToLowerInvariant());
        command.Parameters.AddWithValue("$word", entry.Word);
        if (entry.Source == "pair") command.Parameters.AddWithValue("$replacement", entry.Replacement);
        command.ExecuteNonQuery();
        transaction.Commit();
    }

    public static void Validate(DictionaryItem entry)
    {
        if (string.IsNullOrWhiteSpace(entry.Word) || entry.Word.Length > 4096 || entry.Word.Any(char.IsControl))
            throw new ArgumentException("Enter a word or phrase without control characters (up to 4096 characters).");
        if (!ConfigValidator.ValidLanguageTag(entry.Language.Trim()))
            throw new ArgumentException("Enter a BCP 47 language tag, or und for all languages.");
        if (entry.Source is not ("dictionary" or "pair")) throw new ArgumentException("Choose a dictionary entry or pair rule.");
        if (entry.App.Trim().Length > 260 || entry.App.Any(char.IsControl) || entry.App.IndexOfAny(['/', '\\', ':']) >= 0)
            throw new ArgumentException("App scope must be a process name, such as notepad.exe, or empty for all apps.");
        if (entry.Source == "pair" && (entry.Replacement.Length > 4096 || entry.Replacement.Any(char.IsControl) || entry.Word == entry.Replacement))
            throw new ArgumentException("Pair rules need a different replacement without control characters.");
    }

    /// <summary>Removes the selected exclusion from its source table by persistent identity.</summary>
    public void Delete(DictionaryItem entry)
    {
        using var connection = Open();
        Delete(connection, null, entry);
    }

    /// <summary>Deletes within an optional edit transaction, rejecting unknown rule kinds.</summary>
    private static void Delete(SqliteConnection connection, SqliteTransaction? transaction, DictionaryItem entry)
    {
        using var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = entry.Source switch {
            "dictionary" => "delete from custom_dictionary_entries where id = $id",
            "pair" => "delete from learned_correction_rules where id = $id",
            _ => throw new ArgumentException("Unknown exclusion type.") };
        command.Parameters.AddWithValue("$id", entry.Id);
        command.ExecuteNonQuery();
    }

    /// <summary>Opens local storage and ensures exclusion tables; schema versions belong to the engine.</summary>
    private SqliteConnection Open()
    {
        var directory = Path.GetDirectoryName(databasePath);
        if (!string.IsNullOrEmpty(directory)) Directory.CreateDirectory(directory);
        var connection = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = databasePath }.ToString());
        try
        {
            connection.Open();
            using var command = connection.CreateCommand();
            // The same initial table definitions as the engine. The engine owns schema versions.
            command.CommandText = """
                create table if not exists custom_dictionary_entries (
                    id integer primary key, language_code text not null, app_process_name text,
                    entry text not null, created_at text not null default current_timestamp,
                    unique (language_code, app_process_name, entry));
                create table if not exists learned_correction_rules (
                    id integer primary key,
                    learning_enabled integer not null default 0 check (learning_enabled in (0,1)),
                    original_text text not null, rejected_correction text, rule_type text not null,
                    language_code text, app_process_name text, created_at text not null default current_timestamp);
                """;
            command.ExecuteNonQuery();
            return connection;
        }
        catch { connection.Dispose(); throw; }
    }
}
