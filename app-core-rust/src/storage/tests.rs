use rusqlite::Connection;
use std::{fs, time::SystemTime};

use super::{
    migrations::CURRENT_SCHEMA_VERSION, AppRule, CorrectionMetadata, CustomDictionaryEntry,
    Database, DebugEvent, LanguageOverride, LearnedCorrectionRule,
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
}

#[test]
fn debug_events_drop_typed_text_unless_full_debug_enabled() {
    let database = Database::open_memory().unwrap();
    database
        .debug_events()
        .record(&DebugEvent {
            session_id: Some("session-1".to_owned()),
            event_type: "correction_skipped".to_owned(),
            severity: "debug".to_owned(),
            message: "blocked app".to_owned(),
            typed_text: Some("private words".to_owned()),
            full_debug_enabled: false,
        })
        .unwrap();

    let typed_text: Option<String> = database
        .connection
        .query_row("select typed_text from debug_events", [], |row| row.get(0))
        .unwrap();
    assert!(typed_text.is_none());
}

#[test]
fn clear_logs_removes_correction_metadata_and_debug_events() {
    let database = Database::open_memory().unwrap();
    database
        .correction_metadata()
        .record(&CorrectionMetadata {
            session_id: "session-1".to_owned(),
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
        .record(&DebugEvent {
            session_id: Some("session-1".to_owned()),
            event_type: "api_timeout".to_owned(),
            severity: "warn".to_owned(),
            message: "timeout".to_owned(),
            typed_text: None,
            full_debug_enabled: false,
        })
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
