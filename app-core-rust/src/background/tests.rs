use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{background::paths::RuntimePaths, settings::AppConfig};

use super::{admin, load_or_create_config, BackgroundError, BackgroundRuntime};

#[test]
fn slow_input_worker_discards_stale_batches_at_queue_limit() {
    let worker = super::InputWorker::start(
        AppConfig::default(),
        crate::storage::Database::open_memory().unwrap(),
    )
    .unwrap();
    let (ready_sender, ready) = std::sync::mpsc::channel();
    let (release, release_receiver) = std::sync::mpsc::channel();
    worker.send(super::InputWork::Pause(ready_sender, release_receiver));
    ready
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();

    for _ in 0..=super::INPUT_WORK_QUEUE_LIMIT {
        worker.send(super::InputWork::Events(Vec::new()));
    }
    let (lock, _) = &*worker.queue;
    let queued = lock.lock().unwrap();
    assert_eq!(queued.len(), 1);
    assert!(matches!(queued.front(), Some(super::InputWork::Reset)));
    drop(queued);

    release.send(()).unwrap();
    worker.shutdown();
}

#[test]
fn creates_default_config_when_missing() {
    let root = unique_temp_dir();
    let config_path = root.join("settings.toml");

    let config = load_or_create_config(&config_path).unwrap();

    assert_eq!(config, AppConfig::default());
    assert!(config_path.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn background_runtime_respects_elevation_and_initializes_files() {
    let root = unique_temp_dir();
    let config_path = root.join("settings.toml");
    let database_path = root.join("autofix.sqlite");
    let paths = RuntimePaths::new(
        config_path.clone(),
        database_path.clone(),
        root.join("logs"),
    );

    let elevation = admin::reject_elevated_process();
    let result = BackgroundRuntime::start(paths);
    match elevation {
        Err(BackgroundError::ElevatedProcess) => {
            assert!(matches!(result, Err(BackgroundError::ElevatedProcess)));
            assert!(!root.exists());
        }
        Ok(()) => {
            let runtime = result.unwrap();
            #[cfg(windows)]
            assert_ne!(runtime.components.input_listener.hook_thread_id(), unsafe {
                windows_sys::Win32::System::Threading::GetCurrentThreadId()
            });
            let (reply, response) = std::sync::mpsc::channel();
            runtime
                .components
                .input_worker
                .send(super::InputWork::Probe(reply));
            let worker_id = response
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("input worker did not respond");
            assert_ne!(worker_id, std::thread::current().id());
            runtime.shutdown();

            assert!(config_path.exists());
            assert!(database_path.exists());
            assert!(root.join("logs").exists());
            fs::remove_dir_all(root).unwrap();
        }
        Err(other) => panic!("unexpected elevation check error: {other}"),
    }
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
