use rusqlite::Connection;
use std::{fs, time::SystemTime};

use super::{
    logs::SafeDebugEvent,
    migrations::{self, CURRENT_SCHEMA_VERSION},
    types::{DebugEvent, DebugLogMode, DebugPayload},
    AppRule, CorrectionMetadata, CustomDictionaryEntry, Database, LanguageOverride,
    LearnedCorrectionRule,
};

/// Optional metadata must neither create a missing database nor migrate an existing one.
#[test]
fn metadata_nowait_never_creates_or_migrates_storage() {
    let path = std::env::temp_dir().join(format!(
        "autofix-metadata-{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let metadata = CorrectionMetadata {
        session_id: "1".into(),
        app_process_name: "notepad.exe".into(),
        trigger_type: "manual_shortcut".into(),
        confidence_tier: "high".into(),
        engine_used: "local_rule".into(),
        replacement_method: "none".into(),
        result_reason: "no_correction_needed".into(),
        latency_ms: 0,
    };
    assert!(Database::record_metadata_nowait(&path, &metadata).is_err());
    assert!(!path.exists());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("create table sentinel (value text)")
        .unwrap();
    assert!(Database::record_metadata_nowait(&path, &metadata).is_err());
    let tables: i64 = connection
        .query_row(
            "select count(*) from sqlite_master where type = 'table'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 1);
    drop(connection);
    fs::remove_file(path).unwrap();
}

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
        safety_mode: "auto".into(),
        prose_context_allowed: false,
    };

    database.app_rules().upsert(&rule).unwrap();

    assert!(database.app_rules().list().unwrap().contains(&rule));
}

#[test]
fn upserts_process_only_app_rules_into_one_row() {
    let database = Database::open_memory().unwrap();
    let mut rule = AppRule {
        process_name: "notepad.exe".to_owned(),
        window_title_pattern: None,
        list_behavior: "allowlist".to_owned(),
        manual_shortcut_allowed: true,
        word_count_trigger_allowed: false,
        character_trigger_allowed: false,
        local_engine_allowed: true,
        api_engine_allowed: true,
        safety_mode: "auto".into(),
        prose_context_allowed: false,
    };
    database.app_rules().upsert(&rule).unwrap();

    rule.list_behavior = "blocklist".to_owned();
    rule.manual_shortcut_allowed = false;
    rule.local_engine_allowed = false;
    rule.api_engine_allowed = false;
    database.app_rules().upsert(&rule).unwrap();

    let listed: Vec<_> = database
        .app_rules()
        .list()
        .unwrap()
        .into_iter()
        .filter(|item| item.process_name == "notepad.exe")
        .collect();
    assert_eq!(listed, vec![rule]);
}

#[test]
fn v4_migration_normalizes_legacy_null_process_only_rules() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "
            create table schema_migrations (
                version integer primary key,
                applied_at text not null default current_timestamp
            );
            insert into schema_migrations (version) values (1), (2), (3);

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

            insert into app_rules (
                process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                api_engine_allowed
            ) values
                ('notepad.exe', null, 'allowlist', 1, 0, 0, 1, 1),
                ('notepad.exe', null, 'blocklist', 0, 0, 0, 0, 0);
            ",
        )
        .unwrap();

    migrations::migrate(&connection).unwrap();

    assert_eq!(
        row_count_where(
            &connection,
            "app_rules",
            "process_name = 'notepad.exe' and window_title_pattern = ''"
        ),
        1
    );
    assert_eq!(
        row_count_where(&connection, "app_rules", "window_title_pattern is null"),
        0
    );
    assert_eq!(schema_version(&connection), CURRENT_SCHEMA_VERSION);
}

#[test]
fn seeds_default_app_rules() {
    let database = Database::open_memory().unwrap();
    let rules = database.app_rules().list().unwrap();

    assert!(rules.iter().any(|rule| rule.process_name == "cmd.exe"
        && !rule.manual_shortcut_allowed
        && !rule.word_count_trigger_allowed
        && !rule.character_trigger_allowed));
    assert!(rules.iter().any(|rule| rule.process_name == "code.exe"
        && !rule.manual_shortcut_allowed
        && !rule.word_count_trigger_allowed
        && !rule.character_trigger_allowed));
    assert!(rules
        .iter()
        .any(|rule| rule.process_name == "Bitwarden.exe" && rule.list_behavior == "blocklist"));
}

#[test]
fn safety_migration_preserves_existing_permissions_and_defaults_prose_to_off() {
    let connection = Connection::open_in_memory().unwrap();
    migrations::migrate(&connection).unwrap();
    connection.execute_batch("UPDATE app_rules SET manual_shortcut_allowed=1, character_trigger_allowed=1 WHERE process_name='code.exe'; ALTER TABLE app_rules DROP COLUMN safety_mode; ALTER TABLE app_rules DROP COLUMN prose_context_allowed; DELETE FROM schema_migrations WHERE version=5;").unwrap();
    migrations::migrate(&connection).unwrap();
    migrations::migrate(&connection).unwrap();
    let repository = super::repositories::AppRuleRepository::new(&connection);
    let mut rule = repository
        .list()
        .unwrap()
        .into_iter()
        .find(|rule| rule.process_name == "code.exe")
        .unwrap();
    assert!(rule.manual_shortcut_allowed && rule.character_trigger_allowed);
    assert_eq!(rule.safety_mode, "auto");
    assert!(!rule.prose_context_allowed);
    rule.safety_mode = "code_editor".into();
    rule.prose_context_allowed = true;
    repository.upsert(&rule).unwrap();
    assert!(repository.list().unwrap().contains(&rule));
}

#[test]
fn safety_migration_rolls_back_columns_and_retries_after_marker_failure() {
    for retained_column in [None, Some("safety_mode"), Some("prose_context_allowed")] {
        let connection = Connection::open_in_memory().unwrap();
        migrations::migrate(&connection).unwrap();
        connection
            .execute_batch(
                "UPDATE app_rules SET safety_mode='code_editor', prose_context_allowed=1,
                     manual_shortcut_allowed=1 WHERE process_name='code.exe';
                 DELETE FROM schema_migrations WHERE version=5;
                 CREATE TRIGGER reject_safety_version BEFORE INSERT ON schema_migrations
                 WHEN NEW.version=5 BEGIN SELECT RAISE(ABORT, 'migration failure'); END;",
            )
            .unwrap();
        for column in ["safety_mode", "prose_context_allowed"] {
            if retained_column != Some(column) {
                connection
                    .execute_batch(&format!("ALTER TABLE app_rules DROP COLUMN {column}"))
                    .unwrap();
            }
        }
        let original_columns = table_columns(&connection, "app_rules");
        let original_rows = row_count(&connection, "app_rules");

        let error = migrations::migrate(&connection).unwrap_err();
        assert!(error.to_string().contains("migration failure"));
        assert_eq!(table_columns(&connection, "app_rules"), original_columns);
        assert_eq!(schema_version(&connection), 4);
        assert_eq!(row_count(&connection, "app_rules"), original_rows);
        assert!(connection.is_autocommit());

        connection
            .execute_batch("DROP TRIGGER reject_safety_version")
            .unwrap();
        migrations::migrate(&connection).unwrap();
        migrations::migrate(&connection).unwrap();
        assert_eq!(schema_version(&connection), CURRENT_SCHEMA_VERSION);
        let rule = super::repositories::AppRuleRepository::new(&connection)
            .list()
            .unwrap()
            .into_iter()
            .find(|rule| rule.process_name == "code.exe")
            .unwrap();
        assert!(rule.manual_shortcut_allowed);
        assert_eq!(
            rule.safety_mode,
            if retained_column == Some("safety_mode") {
                "code_editor"
            } else {
                "auto"
            }
        );
        assert_eq!(
            rule.prose_context_allowed,
            retained_column == Some("prose_context_allowed")
        );
    }
}

#[test]
fn deletes_app_rules_by_process_and_title_pattern() {
    let database = Database::open_memory().unwrap();
    let rule = AppRule {
        process_name: "word.exe".to_owned(),
        window_title_pattern: Some("*admin*".to_owned()),
        list_behavior: "blocklist".to_owned(),
        manual_shortcut_allowed: false,
        word_count_trigger_allowed: false,
        character_trigger_allowed: false,
        local_engine_allowed: false,
        api_engine_allowed: false,
        safety_mode: "auto".into(),
        prose_context_allowed: false,
    };
    database.app_rules().upsert(&rule).unwrap();

    assert!(database
        .app_rules()
        .delete("WORD.EXE", Some("*admin*"))
        .unwrap());
    assert!(!database.app_rules().list().unwrap().contains(&rule));
}

#[test]
fn reset_app_rules_restores_seed_defaults() {
    let database = Database::open_memory().unwrap();
    database.app_rules().reset_to_defaults().unwrap();
    let first_reset = database.app_rules().list().unwrap();

    database.app_rules().reset_to_defaults().unwrap();
    let second_reset = database.app_rules().list().unwrap();

    assert_eq!(first_reset, second_reset);
    assert!(first_reset
        .iter()
        .any(|rule| rule.process_name == "cmd.exe"));
}

#[test]
fn failed_app_rule_reset_preserves_all_existing_rules() {
    let database = Database::open_memory().unwrap();
    let mut custom = database.app_rules().list().unwrap().remove(0);
    custom.process_name = "custom.exe".into();
    custom.list_behavior = "blocklist".into();
    database.app_rules().upsert(&custom).unwrap();
    let before = database.app_rules().list().unwrap();
    // Fail after cmd.exe has been inserted, so rollback must undo both the
    // original deletion and an already partially seeded replacement list.
    database
        .connection
        .execute_batch(
            "create trigger reject_default before insert on app_rules
         when new.process_name = 'powershell.exe'
         begin select raise(abort, 'injected seed failure'); end;",
        )
        .unwrap();

    assert!(database.app_rules().reset_to_defaults().is_err());
    assert_eq!(database.app_rules().list().unwrap(), before);

    database
        .connection
        .execute_batch("drop trigger reject_default")
        .unwrap();
    database.app_rules().reset_to_defaults().unwrap();
    let after = database.app_rules().list().unwrap();
    assert!(!after.contains(&custom));
    assert_eq!(after.len(), migrations::DEFAULT_APP_RULES.len());
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
    assert_eq!(
        database
            .custom_dictionary()
            .entries_for_app("word.exe")
            .unwrap(),
        vec!["AutoFix"]
    );
    assert_eq!(
        database
            .custom_dictionary()
            .entries_for_app("code.exe")
            .unwrap(),
        vec!["AutoFix", "crate feature"]
    );
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
fn debug_events_reject_mismatched_mode_and_payload() {
    let database = Database::open_memory().unwrap();
    let result = database.debug_events().record(&DebugEvent {
        session_id: Some("session-1".to_owned()),
        event_type: "correction_skipped".to_owned(),
        severity: "debug".to_owned(),
        message: "bad debug event".to_owned(),
        mode: DebugLogMode::FullText,
        payload: DebugPayload::Redacted {
            label: "typed_text".to_owned(),
        },
    });

    assert!(result.is_err());
    assert_eq!(row_count(&database.connection, "debug_events"), 0);
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

fn row_count_where(connection: &Connection, table_name: &str, predicate: &str) -> i64 {
    connection
        .query_row(
            &format!("select count(*) from {table_name} where {predicate}"),
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn schema_version(connection: &Connection) -> i64 {
    connection
        .query_row("select max(version) from schema_migrations", [], |row| {
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
