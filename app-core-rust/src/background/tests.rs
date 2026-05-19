use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{background::paths::RuntimePaths, settings::AppConfig};

use super::{load_or_create_config, BackgroundRuntime};

#[test]
fn creates_default_config_when_missing() {
    let root = unique_temp_dir();
    let config_path = root.join("config.toml");

    let config = load_or_create_config(&config_path).unwrap();

    assert_eq!(config, AppConfig::default());
    assert!(config_path.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn starts_background_runtime_with_user_config_and_database() {
    let root = unique_temp_dir();
    let config_path = root.join("config.toml");
    let database_path = root.join("autofix.sqlite");
    let paths = RuntimePaths::new(config_path.clone(), database_path.clone());

    let runtime = BackgroundRuntime::start(paths).unwrap();
    runtime.shutdown();

    assert!(config_path.exists());
    assert!(database_path.exists());
    fs::remove_dir_all(root).unwrap();
}

fn unique_temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "autofix-background-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
