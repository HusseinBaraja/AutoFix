use rusqlite::Connection;
use std::{fs, time::SystemTime};

use super::{
    logs::SafeDebugEvent,
    migrations::{self, CURRENT_SCHEMA_VERSION},
    AppRule, CorrectionMetadata, CustomDictionaryEntry, Database, LanguageOverride,
    LearnedCorrectionRule,
};

#[test]
fn opens_sqlite_database_and_runs_migrations() {
    let database = Database::open_memory().unwrap();

    assert!(!database.sqlite_version().unwrap().is_empty());
    assert_eq!(database.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
}

#[test]
fn opens_file_backed_database() {
    let path = std::env::temp_dir().join(format!(
        "autofix-storage-{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let database = Database::open(&path).unwrap();

    assert_eq!(database.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(database);
    fs::remove_file(path).unwrap();
}

#[test]
fn stores_and_lists_app_rules() {
    let database = Database::open_memory().unwrap();
    let rule = AppRule {
        process_name: "notepad.exe".to_owned(),
        window_title_pattern: Some("*draft*".to_owned()),
        list_behavior: "allowlist".to_owned(),
        manual_shortcut_allowed: true,
        word_count_trigger_allowed: false,
        character_trigger_allowed: true,
        local_engine_allowed: true,
        api_engine_allowed: false,
    };

    database.app_rules().upsert(&rule).unwrap();

    assert_eq!(database.app_rules().list().unwrap(), vec![rule]);
}

#[test]
fn dictionary_matches_global_or_app_specific_entries() {
    let database = Database::open_memory().unwrap();
    database
        .custom_dictionary()
        .add_entry(&CustomDictionaryEntry {
            language_code: "en".to_owned(),
            app_process_name: None,
            entry: "AutoFix".to_owned(),
        })
        .unwrap();
    database
        .custom_dictionary()
        .add_entry(&CustomDictionaryEntry {
            language_code: "en".to_owned(),
            app_process_name: Some("code.exe".to_owned()),
            entry: "crate feature".to_owned(),
        })
        .unwrap();

    assert!(database
        .custom_dictionary()
        .contains("en", Some("word.exe"), "AutoFix")
        .unwrap());
    assert!(database
        .custom_dictionary()
        .contains("en", Some("code.exe"), "crate feature")
        .unwrap());
    assert!(!database
        .custom_dictionary()
        .contains("fr", Some("code.exe"), "AutoFix")
        .unwrap());
}

#[test]
fn stores_learned_rules_with_learning_disabled_by_default() {
    let database = Database::open_memory().unwrap();
    database
        .learned_rules()
        .add_rule(&LearnedCorrectionRule {
            learning_enabled: false,
            original_text: "teh".to_owned(),
            rejected_correction: Some("the".to_owned()),
            rule_type: "never_change_x_to_y".to_owned(),
            language_code: Some("en".to_owned()),
            app_process_name: None,
        })
        .unwrap();

    let enabled: bool = database
        .connection
        .query_row(
            "select learning_enabled from learned_correction_rules",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!enabled);
}

#[test]
fn upserts_language_overrides_per_app() {
    let database = Database::open_memory().unwrap();

    database
        .language_overrides()
        .upsert(&LanguageOverride {
            app_process_name: "notepad.exe".to_owned(),
            language_code: "ar".to_owned(),
        })
        .unwrap();

    assert_eq!(
        database
            .language_overrides()
            .find("notepad.exe")
            .unwrap()
            .unwrap(),
        "ar"
    );
}

#[test]
fn correction_metadata_has_no_text_columns() {
    let database = Database::open_memory().unwrap();
    database
        .correction_metadata()
        .record(&CorrectionMetadata {
            session_id: "session-1".to_owned(),
            app_process_name: "notepad.exe".to_owned(),
            trigger_type: "manual_shortcut".to_owned(),
            confidence_tier: "high".to_owned(),
            engine_used: "local".to_owned(),
            replacement_method: "clipboard_restore".to_owned(),
            result_reason: "success".to_owned(),
            latency_ms: 42,
        })
        .unwrap();

    let columns = table_columns(&database.connection, "correction_metadata");
    assert!(!columns.iter().any(|column| column.contains("text")));
    assert!(columns.contains(&"app_process_name".to_owned()));
}

#[test]
fn debug_events_drop_text_unless_full_text_debug_is_explicit() {
    let database = Database::open_memory().unwrap();
    database
        .debug_events()
        .record(
            &SafeDebugEvent::new(
                Some("session-1".to_owned()),
                "correction_skipped",
                "debug",
                "blocked app",
            )
            .redacted("typed_text"),
        )
        .unwrap();

    let typed_text: Option<String> = database
        .connection
        .query_row("select typed_text from debug_events", [], |row| row.get(0))
        .unwrap();
    assert!(typed_text.is_none());
}

#[test]
fn debug_events_store_full_text_only_with_explicit_full_text_payload() {
    let database = Database::open_memory().unwrap();
    database
        .debug_events()
        .record(
            &SafeDebugEvent::new(
                Some("session-1".to_owned()),
                "correction_skipped",
                "debug",
                "full capture",
            )
            .full_text("private words"),
        )
        .unwrap();

    let typed_text: Option<String> = database
        .connection
        .query_row("select typed_text from debug_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(typed_text, Some("private words".to_owned()));
}

#[test]
fn v2_migration_preserves_full_debug_mode() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "
            create table schema_migrations (
                version integer primary key,
                applied_at text not null default current_timestamp
            );
            insert into schema_migrations (version) values (1);

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
            create index idx_debug_events_session on debug_events(session_id, occurred_at);

            insert into debug_events (
                session_id, event_type, severity, message, typed_text, full_debug_enabled
            )
            values
                ('session-1', 'capture', 'debug', 'full', 'private words', 1),
                ('session-1', 'capture', 'debug', 'redacted', null, 0);
            ",
        )
        .unwrap();

    migrations::migrate(&connection).unwrap();

    let modes = debug_modes(&connection);
    assert_eq!(
        modes,
        vec![
            (1, "full_text".to_owned(), Some("private words".to_owned())),
            (2, "redacted".to_owned(), None),
        ]
    );
    assert_column_is_required(&connection, "debug_events", "debug_mode");
    assert!(connection
        .execute(
            "
            insert into debug_events (
                session_id, event_type, severity, message, debug_mode
            )
            values ('session-1', 'capture', 'debug', 'bad mode', 'invalid')
            ",
            [],
        )
        .is_err());
}

#[test]
fn debug_events_are_off_by_default() {
    let database = Database::open_memory().unwrap();
    database
        .debug_events()
        .record(
            &SafeDebugEvent::new(
                Some("session-1".to_owned()),
                "correction_skipped",
                "debug",
                "blocked app",
            )
            .off(),
        )
        .unwrap();

    assert_eq!(row_count(&database.connection, "debug_events"), 0);
}

#[test]
fn clear_logs_removes_correction_metadata_and_debug_events() {
    let database = Database::open_memory().unwrap();
    database
        .correction_metadata()
        .record(&CorrectionMetadata {
            session_id: "session-1".to_owned(),
            app_process_name: "notepad.exe".to_owned(),
            trigger_type: "character".to_owned(),
            confidence_tier: "medium".to_owned(),
            engine_used: "api".to_owned(),
            replacement_method: "selection_replace".to_owned(),
            result_reason: "timeout".to_owned(),
            latency_ms: 700,
        })
        .unwrap();
    database
        .debug_events()
        .record(
            &SafeDebugEvent::new(
                Some("session-1".to_owned()),
                "api_timeout",
                "warn",
                "timeout",
            )
            .redacted("none"),
        )
        .unwrap();

    database.clear_logs().unwrap();

    assert_eq!(row_count(&database.connection, "correction_metadata"), 0);
    assert_eq!(row_count(&database.connection, "debug_events"), 0);
}

fn table_columns(connection: &Connection, table_name: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("pragma table_info({table_name})"))
        .unwrap();
    statement
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap()
}

fn row_count(connection: &Connection, table_name: &str) -> i64 {
    connection
        .query_row(&format!("select count(*) from {table_name}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn debug_modes(connection: &Connection) -> Vec<(i64, String, Option<String>)> {
    let mut statement = connection
        .prepare("select id, debug_mode, typed_text from debug_events order by id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn assert_column_is_required(connection: &Connection, table_name: &str, column_name: &str) {
    let mut statement = connection
        .prepare(&format!("pragma table_info({table_name})"))
        .unwrap();
    let is_required = statement
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let not_null: bool = row.get(3)?;
            Ok((name, not_null))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .into_iter()
        .any(|(name, not_null)| name == column_name && not_null);
    assert!(is_required);
}
