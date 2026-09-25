use std::{
    cell::Cell,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{background::paths::RuntimePaths, settings::AppConfig};

use super::{admin, load_or_create_config, BackgroundError, BackgroundRuntime};

#[test]
fn queued_typing_before_or_during_capture_never_enters_informative_context() {
    for arrives_during_capture in [false, true] {
        let sequence = Cell::new(1_u64);
        let limits = AppConfig::default().context;
        let mut manager = super::SessionManager::new(limits.clone());
        let target = super::target::FocusedTarget {
            process_id: 1,
            process_name: "notepad.exe".into(),
            window_handle: 1,
            window_title: "Notes".into(),
            focused_element_id: None,
            is_elevated: false,
            is_password_or_protected: false,
            is_hidden_or_unavailable: false,
            field_safety_known: true,
            is_secure_desktop: false,
            is_lock_screen: false,
            is_credential_dialog: false,
        };
        manager.focus(&target);
        manager.input(super::typing::TypedInput::Text("a".into()));
        if !arrives_during_capture {
            sequence.set(2);
        }
        let preceding = super::capture_if_current(
            1,
            || sequence.get(),
            || {
                sequence.set(2);
                Some("aa".to_owned())
            },
        );
        if let Some(preceding) = preceding {
            let context =
                super::context_capture::captured_context(preceding.as_deref(), "a", &limits);
            manager.set_informative_context(context);
        }
        manager.input(super::typing::TypedInput::Text("a".into()));
        let session = manager.active().unwrap();
        assert_eq!(session.informative_context(), "");
        assert_eq!(session.executable_context(), "aa");
    }
}

#[test]
fn delayed_key_from_previous_focus_cannot_enter_session() {
    let config = AppConfig::default();
    let mut processor = super::InputProcessor {
        session_manager: super::SessionManager::new(config.context.clone()),
        config,
        database: crate::storage::Database::open_memory().unwrap(),
    };
    let target = super::target::FocusedTarget {
        process_id: 1,
        process_name: "notepad.exe".into(),
        window_handle: 1,
        window_title: "Notes".into(),
        focused_element_id: None,
        is_elevated: false,
        is_password_or_protected: false,
        is_hidden_or_unavailable: false,
        field_safety_known: true,
        is_secure_desktop: false,
        is_lock_screen: false,
        is_credential_dialog: false,
    };
    processor.session_manager.focus(&target);
    processor
        .session_manager
        .input(super::typing::TypedInput::Text("typed".into()));
    processor.process_input(vec![super::InputEvent::Key(
        super::input_listener::stale_key_for_test(),
    )]);
    assert!(processor.session_manager.active().is_none());
}

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
