use super::*;
use crate::{
    settings::{load_config, save_config, AppConfig},
    storage::AppPolicyGuard,
};
use rusqlite::Connection;
use std::{fs, time::SystemTime};

fn fixture(committed: bool) -> (PathBuf, Connection, Vec<u8>, Vec<u8>) {
    let root = std::env::temp_dir().join(format!(
        "autofix-import-recovery-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.toml");
    let mut config = AppConfig::default();
    let original = crate::settings::toml_io::config_to_toml(&config)
        .unwrap()
        .into_bytes();
    config.triggers.word_count = 22;
    let imported = crate::settings::toml_io::config_to_toml(&config)
        .unwrap()
        .into_bytes();
    let database = Connection::open(root.join("autofix.sqlite")).unwrap();
    database
        .execute_batch(
            "create table settings_import_recovery (
        id integer primary key check(id = 1), original_settings blob not null,
        imported_settings blob not null, committed integer not null check(committed in (0,1)))",
        )
        .unwrap();
    database
        .execute(
            "insert into settings_import_recovery values (1, ?1, ?2, ?3)",
            rusqlite::params![original, imported, committed],
        )
        .unwrap();
    (path, database, original, imported)
}

/// Native startup reads the same durable decision as WPF, restoring even a missing or corrupted file.
#[test]
fn native_reads_reconcile_both_import_decisions_before_loading() {
    for committed in [false, true] {
        let (path, database, original, imported) = fixture(committed);
        fs::write(&path, "partially persisted file").unwrap();
        assert_eq!(
            load_config(&path).unwrap().triggers.word_count,
            if committed { 22 } else { 10 }
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            if committed { imported } else { original }
        );
        assert!(!pending(&database).unwrap());
        assert!(!recover(&path).unwrap());
        drop(database);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

/// First-start initialization repairs a committed import when its settings file did not survive shutdown.
#[test]
fn startup_recovery_precedes_default_creation() {
    let (path, database, _, imported) = fixture(true);
    assert!(!path.exists());
    assert_eq!(
        crate::settings::load_or_create_config(&path)
            .unwrap()
            .triggers
            .word_count,
        22
    );
    assert_eq!(fs::read(&path).unwrap(), imported);
    assert!(!pending(&database).unwrap());
    drop(database);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

/// Both outbound admission and replacement reservation fail closed while any import record survives.
#[test]
fn correction_policy_is_denied_until_import_recovery() {
    for committed in [false, true] {
        let (path, database, _, _) = fixture(committed);
        let database_path = path.parent().unwrap().join("autofix.sqlite");
        assert!(AppPolicyGuard::read_rules_nowait(&database_path).is_err());
        assert!(AppPolicyGuard::acquire(&database_path).is_err());
        recover(&path).unwrap();
        assert!(AppPolicyGuard::acquire(&database_path).is_ok());
        drop(database);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

/// IPC edits first recover the committed import and preserve its unrelated settings.
#[test]
fn fresh_edits_preserve_recovered_imported_values() {
    let (path, database, _, _) = fixture(true);
    let edited = crate::settings::edit_config(&path, |config| {
        config.general.start_with_windows = true;
        Ok(())
    })
    .unwrap();
    assert_eq!(edited.triggers.word_count, 22);
    assert!(edited.general.start_with_windows);
    drop(database);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

/// A conflicting Windows handle prevents reconciliation without deleting the only durable recovery record.
#[cfg(windows)]
#[test]
fn failed_recovery_and_contended_lock_preserve_the_decision() {
    use std::os::windows::fs::OpenOptionsExt;
    let (path, database, _, _) = fixture(false);
    fs::write(&path, "imported").unwrap();
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(load_config(&path).is_err());
    assert!(pending(&database).unwrap());
    drop(locked);
    let access = acquire(&path).unwrap();
    assert!(load_config(&path).is_err());
    assert!(save_config(&path, &AppConfig::default()).is_err());
    drop(access);
    assert_eq!(load_config(&path).unwrap().triggers.word_count, 10);
    drop(database);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
