use rusqlite::{Connection, Result};

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 2;

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
        alter table debug_events add column debug_mode text not null default 'redacted' check (debug_mode in ('redacted', 'full_text'));
        alter table debug_events add column redacted_label text;
        ",
    )
}
