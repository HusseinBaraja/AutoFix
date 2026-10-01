use std::{
    cell::Cell,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{background::paths::RuntimePaths, settings::AppConfig};

use super::{admin, load_or_create_config, BackgroundError, BackgroundRuntime};

/// Exercise both dispatch checks when hook input arrives before or during the slow gate.
#[test]
fn frozen_dispatch_survives_typing_but_manual_and_moved_requests_are_rejected() {
    for frozen in [false, true] {
        for moved in [false, true] {
            for during_gate in [false, true] {
                let config = AppConfig::default();
                let mut processor = super::InputProcessor {
                    learner: crate::dictionary::Learner::default(),
                    pipeline: super::CorrectionPipeline::new().unwrap(),
                    processed_input_sequence: 12,
                    session_manager: super::SessionManager::new(config.context.clone()),
                    config,
                    database: crate::storage::Database::open_memory().unwrap(),
                };
                let target = dispatch_target();
                processor.session_manager.focus(&target);
                processor
                    .session_manager
                    .input(super::typing::TypedInput::Text("First.".into()));
                let mut pending = Vec::new();
                processor.track_input(super::typing::TypedInput::Text(".".into()), &mut pending);
                let mut request = pending.remove(0).request;
                if !frozen {
                    request.trigger = super::TriggerKind::ManualShortcut;
                    processor
                        .session_manager
                        .active_mut()
                        .unwrap()
                        .restore_pending();
                    request.pending_segment_id = None;
                    request.versions = processor.session_manager.active().unwrap().versions();
                }
                let stamp = super::InputStamp {
                    position: 7,
                    sequence: 12,
                };
                let changed = super::InputStamp {
                    position: if moved { 8 } else { 7 },
                    sequence: 13,
                };
                let current = Cell::new(if during_gate { stamp } else { changed });
                let dispatched = processor.dispatch_trigger_with(
                    request,
                    stamp,
                    || current.get(),
                    |_, _, _| {
                        current.set(changed);
                        super::SecurityDecision::Allowed {
                            target: target.clone(),
                        }
                    },
                );
                assert_eq!(dispatched, frozen && !moved);
                if dispatched {
                    processor
                        .session_manager
                        .input(super::typing::TypedInput::Text(" next".into()));
                    finish_unchanged_dispatch(&mut processor, &target, changed, "First.. next");
                    let session = processor.session_manager.active().unwrap();
                    assert_eq!(session.informative_context(), "First..");
                    assert_eq!(session.editable_context(), " next");
                }
            }
        }
    }
}

/// Denying a newer dispatch restores its text and preserves an older admitted request.
#[test]
fn failed_frozen_dispatch_preserves_older_work_and_releases_newer_reservations() {
    let mut config = AppConfig::default();
    config.context.pending_queue_size = 3;
    let mut processor = super::InputProcessor {
        learner: crate::dictionary::Learner::default(),
        pipeline: super::CorrectionPipeline::new().unwrap(),
        processed_input_sequence: 12,
        session_manager: super::SessionManager::new(config.context.clone()),
        config,
        database: crate::storage::Database::open_memory().unwrap(),
    };
    let target = dispatch_target();
    processor.session_manager.focus(&target);
    let mut pending = Vec::new();
    for text in ["First.", " Second.", " Third."] {
        processor.track_input(super::typing::TypedInput::Text(text.into()), &mut pending);
    }
    let stamp = super::InputStamp {
        position: 7,
        sequence: 12,
    };
    let first = pending.remove(0).request;
    let first_id = first.pending_segment_id.unwrap();
    assert!(processor.dispatch_trigger_with(
        first,
        stamp,
        || stamp,
        |_, _, _| {
            super::SecurityDecision::Allowed {
                target: target.clone(),
            }
        }
    ));
    assert!(!processor.dispatch_trigger_with(
        pending.remove(0).request,
        stamp,
        || stamp,
        |_, _, _| {
            super::SecurityDecision::Blocked {
                reason: super::security::BlockReason::AppPolicyUnavailable,
                target: None,
            }
        }
    ));
    let session = processor.session_manager.active().unwrap();
    assert!(session.pending_matches(first_id, "First."));
    assert!(!pending[0].matches_session(session));
    assert_eq!(session.editable_context(), " Second. Third.");
    finish_unchanged_dispatch(&mut processor, &target, stamp, "First. Second. Third.");
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .informative_context(),
        "First."
    );
}

/// Wait for worker publication with a bounded deadline instead of assuming a scheduling delay.
fn finish_unchanged_dispatch(
    processor: &mut super::InputProcessor,
    target: &super::target::FocusedTarget,
    stamp: super::InputStamp,
    live_text: &str,
) {
    let started = std::time::Instant::now();
    loop {
        processor.pipeline.wait_manual();
        if processor.pipeline.finish(
            &mut processor.session_manager,
            &processor.config.context,
            || stamp,
            |_| Some(target.clone()),
            |_, _| Some(live_text.into()),
            |_, _, _| panic!("unchanged text must not be replaced"),
        ) {
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "dispatch did not complete"
        );
    }
}

/// An ordinary, safely identified text control for dispatch regression tests.
fn dispatch_target() -> super::target::FocusedTarget {
    super::target::FocusedTarget {
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
    }
}

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
        learner: crate::dictionary::Learner::default(),
        pipeline: super::CorrectionPipeline::new().unwrap(),
        processed_input_sequence: super::input_listener::current_input_sequence(),
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

/// A punctuation trigger freezes the context even when more keys share its batch.
#[test]
fn later_character_trigger_keeps_full_editable_snapshot() {
    let config = AppConfig::default();
    let mut processor = super::InputProcessor {
        learner: crate::dictionary::Learner::default(),
        pipeline: super::CorrectionPipeline::new().unwrap(),
        processed_input_sequence: super::input_listener::current_input_sequence(),
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
        .input(super::typing::TypedInput::Text("First. Next".into()));
    let mut pending = Vec::new();
    processor.track_input(super::typing::TypedInput::Text(".".into()), &mut pending);

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request.executable_context, "First. Next.");
    assert_eq!(pending[0].editable_snapshot, "First. Next.");
    assert!(pending[0].matches_session(processor.session_manager.active().unwrap()));

    processor
        .session_manager
        .input(super::typing::TypedInput::Text(" More".into()));
    assert!(pending[0].matches_session(processor.session_manager.active().unwrap()));
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .editable_context(),
        " More"
    );
}

#[test]
fn automatic_triggers_in_one_batch_obey_capacity_and_overflow_policy() {
    for policy in [
        crate::settings::PendingQueueFullBehavior::SkipNew,
        crate::settings::PendingQueueFullBehavior::CancelOldest,
        crate::settings::PendingQueueFullBehavior::MergeNewest,
    ] {
        let mut config = AppConfig::default();
        config.context.pending_queue_full_behavior = policy;
        let mut processor = super::InputProcessor {
            learner: crate::dictionary::Learner::default(),
            pipeline: super::CorrectionPipeline::new().unwrap(),
            processed_input_sequence: super::input_listener::current_input_sequence(),
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
        let mut pending = Vec::new();
        processor.track_input(
            super::typing::TypedInput::Text("first.".into()),
            &mut pending,
        );
        processor.track_input(
            super::typing::TypedInput::Text(" next.".into()),
            &mut pending,
        );
        let session = processor.session_manager.active().unwrap();
        let valid: Vec<_> = pending
            .iter()
            .filter(|request| request.matches_session(session))
            .collect();
        match policy {
            crate::settings::PendingQueueFullBehavior::SkipNew => {
                assert_eq!(valid.len(), 1);
                assert_eq!(valid[0].request.executable_context, "first.");
                assert_eq!(session.editable_context(), " next.");
            }
            crate::settings::PendingQueueFullBehavior::CancelOldest => {
                assert_eq!(valid.len(), 1);
                assert_eq!(valid[0].request.executable_context, " next.");
                assert_eq!(valid[0].request.informative_context, "first.");
                assert_eq!(session.editable_context(), "");
            }
            crate::settings::PendingQueueFullBehavior::MergeNewest => {
                assert!(valid.is_empty());
                assert_eq!(session.editable_context(), "first. next.");
                processor.track_input(
                    super::typing::TypedInput::Text(" last.".into()),
                    &mut pending,
                );
                let session = processor.session_manager.active().unwrap();
                assert!(pending.last().unwrap().matches_session(session));
                assert_eq!(
                    pending.last().unwrap().request.executable_context,
                    "first. next. last."
                );
            }
        }
    }
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
