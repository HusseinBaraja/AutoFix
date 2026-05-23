using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Models;
using Microsoft.Data.Sqlite;
using System.IO;

namespace AutoFix.SettingsUi.Settings;

public sealed class AppRuleStorage
{
    public AppRuleStorage() : this(DefaultDatabasePath())
    {
    }

    public AppRuleStorage(string databasePath)
    {
        DatabasePath = databasePath;
    }

    public string DatabasePath { get; }

    public IReadOnlyList<AppRuleItem> List()
    {
        using var connection = OpenMigratedConnection();
        using var command = connection.CreateCommand();
        command.CommandText =
            """
            select process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                   word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                   api_engine_allowed
            from app_rules
            order by process_name, window_title_pattern
            """;

        using var reader = command.ExecuteReader();
        var rules = new List<AppRuleItem>();
        while (reader.Read())
        {
            rules.Add(new AppRuleItem
            {
                ProcessName = reader.GetString(0),
                WindowTitlePattern = reader.IsDBNull(1) ? "" : reader.GetString(1),
                ListBehavior = reader.GetString(2),
                ManualShortcutAllowed = reader.GetBoolean(3),
                WordCountTriggerAllowed = reader.GetBoolean(4),
                CharacterTriggerAllowed = reader.GetBoolean(5),
                LocalEngineAllowed = reader.GetBoolean(6),
                ApiEngineAllowed = reader.GetBoolean(7),
            });
        }

        return rules;
    }

    public void Upsert(AppRuleItem rule)
    {
        Validate(rule);
        using var connection = OpenMigratedConnection();
        using var command = connection.CreateCommand();
        command.CommandText =
            """
            insert into app_rules (
                process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                api_engine_allowed
            ) values ($process_name, $window_title_pattern, $list_behavior, $manual, $word_count, $character, $local, $api)
            on conflict(process_name, window_title_pattern) do update set
                list_behavior = excluded.list_behavior,
                manual_shortcut_allowed = excluded.manual_shortcut_allowed,
                word_count_trigger_allowed = excluded.word_count_trigger_allowed,
                character_trigger_allowed = excluded.character_trigger_allowed,
                local_engine_allowed = excluded.local_engine_allowed,
                api_engine_allowed = excluded.api_engine_allowed,
                updated_at = current_timestamp
            """;
        BindRule(command, rule);
        command.ExecuteNonQuery();
    }

    public bool Delete(string processName, string? windowTitlePattern)
    {
        using var connection = OpenMigratedConnection();
        using var command = connection.CreateCommand();
        command.CommandText =
            """
            delete from app_rules
            where lower(process_name) = lower($process_name)
              and (window_title_pattern is $window_title_pattern or window_title_pattern = $window_title_pattern)
            """;
        command.Parameters.AddWithValue("$process_name", processName);
        command.Parameters.AddWithValue("$window_title_pattern", EmptyWhenBlank(windowTitlePattern));
        return command.ExecuteNonQuery() > 0;
    }

    public IReadOnlyList<AppRuleItem> ResetDefaults()
    {
        using var connection = OpenMigratedConnection();
        using var transaction = connection.BeginTransaction();
        using (var delete = connection.CreateCommand())
        {
            delete.Transaction = transaction;
            delete.CommandText = "delete from app_rules";
            delete.ExecuteNonQuery();
        }

        foreach (var rule in DefaultRules())
        {
            using var insert = connection.CreateCommand();
            insert.Transaction = transaction;
            insert.CommandText =
                """
                insert into app_rules (
                    process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                    word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                    api_engine_allowed
                ) values ($process_name, $window_title_pattern, $list_behavior, $manual, $word_count, $character, $local, $api)
                """;
            BindRule(insert, rule);
            insert.ExecuteNonQuery();
        }

        transaction.Commit();
        return List();
    }

    public static AppRuleDto ToDto(AppRuleItem rule) => new(
        rule.ProcessName.Trim(),
        string.IsNullOrWhiteSpace(rule.WindowTitlePattern) ? null : rule.WindowTitlePattern.Trim(),
        rule.ListBehavior,
        rule.ManualShortcutAllowed,
        rule.WordCountTriggerAllowed,
        rule.CharacterTriggerAllowed,
        rule.LocalEngineAllowed,
        rule.ApiEngineAllowed);

    public static AppRuleItem FromDto(AppRuleDto rule) => new()
    {
        ProcessName = rule.ProcessName,
        WindowTitlePattern = rule.WindowTitlePattern ?? "",
        ListBehavior = rule.ListBehavior,
        ManualShortcutAllowed = rule.ManualShortcutAllowed,
        WordCountTriggerAllowed = rule.WordCountTriggerAllowed,
        CharacterTriggerAllowed = rule.CharacterTriggerAllowed,
        LocalEngineAllowed = rule.LocalEngineAllowed,
        ApiEngineAllowed = rule.ApiEngineAllowed,
    };

    public static void Validate(AppRuleItem rule)
    {
        if (string.IsNullOrWhiteSpace(rule.ProcessName))
        {
            throw new ArgumentException("process_name must not be empty");
        }

        if (rule.ListBehavior is not ("allowlist" or "blocklist"))
        {
            throw new ArgumentException("list_behavior must be allowlist or blocklist");
        }
    }

    private SqliteConnection OpenMigratedConnection()
    {
        var directory = Path.GetDirectoryName(DatabasePath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var connection = new SqliteConnection($"Data Source={DatabasePath}");
        connection.Open();
        using var command = connection.CreateCommand();
        command.CommandText =
            """
            create table if not exists app_rules (
                id integer primary key,
                process_name text not null,
                window_title_pattern text not null default '',
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
            """;
        command.ExecuteNonQuery();
        NormalizeProcessOnlyRules(connection);
        return connection;
    }

    private static void BindRule(SqliteCommand command, AppRuleItem rule)
    {
        command.Parameters.AddWithValue("$process_name", rule.ProcessName.Trim());
        command.Parameters.AddWithValue("$window_title_pattern", EmptyWhenBlank(rule.WindowTitlePattern));
        command.Parameters.AddWithValue("$list_behavior", rule.ListBehavior);
        command.Parameters.AddWithValue("$manual", rule.ManualShortcutAllowed);
        command.Parameters.AddWithValue("$word_count", rule.WordCountTriggerAllowed);
        command.Parameters.AddWithValue("$character", rule.CharacterTriggerAllowed);
        command.Parameters.AddWithValue("$local", rule.LocalEngineAllowed);
        command.Parameters.AddWithValue("$api", rule.ApiEngineAllowed);
    }

    private static string EmptyWhenBlank(string? value) =>
        string.IsNullOrWhiteSpace(value) ? "" : value.Trim();

    private static void NormalizeProcessOnlyRules(SqliteConnection connection)
    {
        using var deleteDuplicates = connection.CreateCommand();
        deleteDuplicates.CommandText =
            """
            delete from app_rules
            where window_title_pattern is null
              and id not in (
                  select max(id)
                  from app_rules
                  where window_title_pattern is null
                  group by lower(process_name)
              )
            """;
        deleteDuplicates.ExecuteNonQuery();

        using var normalizeNulls = connection.CreateCommand();
        normalizeNulls.CommandText = "update app_rules set window_title_pattern = '' where window_title_pattern is null";
        normalizeNulls.ExecuteNonQuery();
    }

    private static string DefaultDatabasePath()
    {
        var root = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        if (string.IsNullOrWhiteSpace(root))
        {
            root = Environment.CurrentDirectory;
        }

        return Path.Combine(root, "AutoFix", "autofix.sqlite");
    }

    private static IReadOnlyList<AppRuleItem> DefaultRules() =>
    [
        Terminal("cmd.exe"),
        Terminal("powershell.exe"),
        Terminal("pwsh.exe"),
        Terminal("windowsterminal.exe"),
        Terminal("wt.exe"),
        Terminal("conhost.exe"),
        Editor("code.exe"),
        Editor("code-insiders.exe"),
        Editor("devenv.exe"),
        Editor("rider64.exe"),
        Editor("idea64.exe"),
        Editor("pycharm64.exe"),
        Editor("webstorm64.exe"),
        Editor("clion64.exe"),
        Editor("notepad++.exe"),
        Editor("sublime_text.exe"),
        Editor("vim.exe"),
        Editor("nvim.exe"),
        Block("mstsc.exe"),
        Block("vmconnect.exe"),
        Block("vmware.exe"),
        Block("vmware-vmx.exe"),
        Block("VirtualBoxVM.exe"),
        Block("TeamViewer.exe"),
        Block("AnyDesk.exe"),
        Block("steam.exe"),
        Block("epicgameslauncher.exe"),
        Block("battle.net.exe"),
        Block("riotclientservices.exe"),
        Block("1Password.exe"),
        Block("Bitwarden.exe"),
        Block("KeePass.exe"),
        Block("KeePassXC.exe"),
        Block("LastPass.exe"),
        Block("Dashlane.exe"),
        Block("NordPass.exe"),
        Block("Proton Pass.exe"),
        Block("regedit.exe"),
        Block("regedt32.exe"),
        Block("taskmgr.exe"),
        Block("services.exe"),
        Block("mmc.exe"),
        Block("consent.exe"),
        Block("CredentialUI.exe"),
    ];

    private static AppRuleItem Terminal(string processName) => new()
    {
        ProcessName = processName,
        ListBehavior = "allowlist",
        ManualShortcutAllowed = false,
        WordCountTriggerAllowed = false,
        CharacterTriggerAllowed = false,
        LocalEngineAllowed = true,
        ApiEngineAllowed = true,
    };

    private static AppRuleItem Editor(string processName) => new()
    {
        ProcessName = processName,
        ListBehavior = "allowlist",
        ManualShortcutAllowed = true,
        WordCountTriggerAllowed = false,
        CharacterTriggerAllowed = false,
        LocalEngineAllowed = true,
        ApiEngineAllowed = true,
    };

    private static AppRuleItem Block(string processName) => new()
    {
        ProcessName = processName,
        ListBehavior = "blocklist",
        ManualShortcutAllowed = false,
        WordCountTriggerAllowed = false,
        CharacterTriggerAllowed = false,
        LocalEngineAllowed = false,
        ApiEngineAllowed = false,
    };
}
