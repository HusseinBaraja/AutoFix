use rusqlite::{Connection, Result};

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 3;

pub(super) struct DefaultAppRule {
    pub(super) process_name: &'static str,
    pub(super) window_title_pattern: Option<&'static str>,
    pub(super) list_behavior: &'static str,
    pub(super) manual_shortcut_allowed: bool,
    pub(super) word_count_trigger_allowed: bool,
    pub(super) character_trigger_allowed: bool,
    pub(super) local_engine_allowed: bool,
    pub(super) api_engine_allowed: bool,
}

pub(super) const DEFAULT_APP_RULES: &[DefaultAppRule] = &[
    terminal_rule("cmd.exe"),
    terminal_rule("powershell.exe"),
    terminal_rule("pwsh.exe"),
    terminal_rule("windowsterminal.exe"),
    terminal_rule("wt.exe"),
    terminal_rule("conhost.exe"),
    editor_rule("code.exe"),
    editor_rule("code-insiders.exe"),
    editor_rule("devenv.exe"),
    editor_rule("rider64.exe"),
    editor_rule("idea64.exe"),
    editor_rule("pycharm64.exe"),
    editor_rule("webstorm64.exe"),
    editor_rule("clion64.exe"),
    editor_rule("notepad++.exe"),
    editor_rule("sublime_text.exe"),
    editor_rule("vim.exe"),
    editor_rule("nvim.exe"),
    block_all_rule("mstsc.exe"),
    block_all_rule("vmconnect.exe"),
    block_all_rule("vmware.exe"),
    block_all_rule("vmware-vmx.exe"),
    block_all_rule("VirtualBoxVM.exe"),
    block_all_rule("TeamViewer.exe"),
    block_all_rule("AnyDesk.exe"),
    block_all_rule("steam.exe"),
    block_all_rule("epicgameslauncher.exe"),
    block_all_rule("battle.net.exe"),
    block_all_rule("riotclientservices.exe"),
    block_all_rule("1Password.exe"),
    block_all_rule("Bitwarden.exe"),
    block_all_rule("KeePass.exe"),
    block_all_rule("KeePassXC.exe"),
    block_all_rule("LastPass.exe"),
    block_all_rule("Dashlane.exe"),
    block_all_rule("NordPass.exe"),
    block_all_rule("Proton Pass.exe"),
    block_all_rule("regedit.exe"),
    block_all_rule("regedt32.exe"),
    block_all_rule("taskmgr.exe"),
    block_all_rule("services.exe"),
    block_all_rule("mmc.exe"),
    block_all_rule("consent.exe"),
    block_all_rule("CredentialUI.exe"),
];

const fn terminal_rule(process_name: &'static str) -> DefaultAppRule {
    DefaultAppRule {
        process_name,
        window_title_pattern: None,
        list_behavior: "allowlist",
        manual_shortcut_allowed: false,
        word_count_trigger_allowed: false,
        character_trigger_allowed: false,
        local_engine_allowed: true,
        api_engine_allowed: true,
    }
}

const fn editor_rule(process_name: &'static str) -> DefaultAppRule {
    DefaultAppRule {
        process_name,
        window_title_pattern: None,
        list_behavior: "allowlist",
        manual_shortcut_allowed: true,
        word_count_trigger_allowed: false,
        character_trigger_allowed: false,
        local_engine_allowed: true,
        api_engine_allowed: true,
    }
}

const fn block_all_rule(process_name: &'static str) -> DefaultAppRule {
    DefaultAppRule {
        process_name,
        window_title_pattern: None,
        list_behavior: "blocklist",
        manual_shortcut_allowed: false,
        word_count_trigger_allowed: false,
        character_trigger_allowed: false,
        local_engine_allowed: false,
        api_engine_allowed: false,
    }
}

pub(super) fn migrate(connection: &Connection) -> Result<()> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.execute_batch(
        "
        create table if not exists schema_migrations (
            version integer primary key,
            applied_at text not null default current_timestamp
        );
        ",
    )?;

    let version = current_version(connection)?;
    if version < 1 {
        migrate_to_v1(connection)?;
        connection.execute("insert into schema_migrations (version) values (1)", [])?;
    }
    if version < 2 {
        migrate_to_v2(connection)?;
        connection.execute("insert into schema_migrations (version) values (2)", [])?;
    }
    if version < 3 {
        seed_default_app_rules(connection)?;
        connection.execute("insert into schema_migrations (version) values (3)", [])?;
    }

    Ok(())
}

pub(super) fn seed_default_app_rules(connection: &Connection) -> Result<()> {
    let mut statement = connection.prepare(
        "
        insert or ignore into app_rules (
            process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
            word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
            api_engine_allowed
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ",
    )?;

    for rule in DEFAULT_APP_RULES {
        statement.execute((
            rule.process_name,
            rule.window_title_pattern,
            rule.list_behavior,
            rule.manual_shortcut_allowed,
            rule.word_count_trigger_allowed,
            rule.character_trigger_allowed,
            rule.local_engine_allowed,
            rule.api_engine_allowed,
        ))?;
    }

    Ok(())
}

fn current_version(connection: &Connection) -> Result<i64> {
    connection.query_row(
        "select coalesce(max(version), 0) from schema_migrations",
        [],
        |row| row.get(0),
    )
}

fn migrate_to_v1(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
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

        create table custom_dictionary_entries (
            id integer primary key,
            language_code text not null,
            app_process_name text,
            entry text not null,
            created_at text not null default current_timestamp,
            unique (language_code, app_process_name, entry)
        );

        create table learned_correction_rules (
            id integer primary key,
            learning_enabled integer not null default 0 check (learning_enabled in (0, 1)),
            original_text text not null,
            rejected_correction text,
            rule_type text not null,
            language_code text,
            app_process_name text,
            created_at text not null default current_timestamp
        );

        create table language_overrides (
            id integer primary key,
            app_process_name text not null unique,
            language_code text not null,
            created_at text not null default current_timestamp,
            updated_at text not null default current_timestamp
        );

        create table correction_metadata (
            id integer primary key,
            occurred_at text not null default current_timestamp,
            session_id text not null,
            trigger_type text not null,
            confidence_tier text not null,
            engine_used text not null,
            replacement_method text not null,
            result_reason text not null,
            latency_ms integer not null check (latency_ms >= 0)
        );

        create table debug_events (
            id integer primary key,
            occurred_at text not null default current_timestamp,
            session_id text,
            event_type text not null,
            severity text not null,
            message text not null,
            typed_text text,
            full_debug_enabled integer not null default 0 check (full_debug_enabled in (0, 1)),
            check (full_debug_enabled = 1 or typed_text is null)
        );

        create index idx_app_rules_process on app_rules(process_name);
        create index idx_dictionary_lookup on custom_dictionary_entries(language_code, app_process_name, entry);
        create index idx_learned_rules_lookup on learned_correction_rules(rule_type, app_process_name, language_code);
        create index idx_correction_metadata_session on correction_metadata(session_id, occurred_at);
        create index idx_debug_events_session on debug_events(session_id, occurred_at);
        ",
    )
}

fn migrate_to_v2(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        alter table correction_metadata add column app_process_name text not null default 'unknown';
        alter table debug_events add column debug_mode text;
        alter table debug_events add column redacted_label text;
        update debug_events
        set debug_mode = case
            when full_debug_enabled = 1 then 'full_text'
            else 'redacted'
        end
        where debug_mode is null;

        create table debug_events_v2 (
            id integer primary key,
            occurred_at text not null default current_timestamp,
            session_id text,
            event_type text not null,
            severity text not null,
            message text not null,
            typed_text text,
            full_debug_enabled integer not null default 0 check (full_debug_enabled in (0, 1)),
            debug_mode text not null check (debug_mode in ('redacted', 'full_text')),
            redacted_label text,
            check (full_debug_enabled = 1 or typed_text is null)
        );

        insert into debug_events_v2 (
            id, occurred_at, session_id, event_type, severity, message, typed_text,
            full_debug_enabled, debug_mode, redacted_label
        )
        select
            id, occurred_at, session_id, event_type, severity, message, typed_text,
            full_debug_enabled, debug_mode, redacted_label
        from debug_events;

        drop table debug_events;
        alter table debug_events_v2 rename to debug_events;
        create index idx_debug_events_session on debug_events(session_id, occurred_at);
        ",
    )
}
