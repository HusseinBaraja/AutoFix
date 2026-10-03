use super::*;
use crate::background::{
    context_capture::SelectionCapture,
    security,
    target::FocusedElementId,
    triggers,
    typing::{MovementSignal, TypedInput},
    InputProcessor, PendingTrigger,
};
use std::{cell::Cell, sync::mpsc, time::Instant};

mod character;
mod manual;
mod word_count;

/// Build the runtime input owner for automatic-trigger flow tests.
fn processor(config: AppConfig, pipeline: CorrectionPipeline) -> InputProcessor {
    let mut processor = InputProcessor {
        feedback: crate::background::feedback::Feedback::default(),
        learner: crate::dictionary::Learner::default(),
        pipeline,
        processed_input_sequence: STAMP.sequence,
        session_manager: SessionManager::new(config.context.clone()),
        config,
        database: crate::storage::Database::open_memory().unwrap(),
    };
    processor.session_manager.focus(&target());
    processor
}

/// Send translated keys individually, preserving the real trigger boundary.
fn type_text(processor: &mut InputProcessor, text: &str) -> Vec<PendingTrigger> {
    let mut pending = Vec::new();
    for character in text.chars() {
        processor.track_input(TypedInput::Text(character.to_string()), &mut pending);
    }
    pending
}

/// Dispatch with real app rules and a deterministic focused target.
fn dispatch(processor: &mut InputProcessor, request: CorrectionRequest) -> bool {
    processor.dispatch_trigger_with(
        request,
        STAMP,
        || STAMP,
        |trigger, config, database| {
            security::check_detection(
                trigger,
                config,
                &database.app_rules().list().unwrap(),
                crate::background::target::TargetDetection::Available(target()),
            )
        },
    )
}

const STAMP: InputStamp = InputStamp {
    position: 7,
    sequence: 12,
};

/// Both routes refuse restricted content before dispatch; refusal preserves session ownership.
#[test]
fn restricted_text_never_reaches_either_correction_engine() {
    let path = std::env::temp_dir().join(format!(
        "autofix-text-safety-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let database = crate::storage::Database::open(&path).unwrap();
    for process in ["cmd.exe", "code.exe"] {
        let mut rule = database
            .app_rules()
            .list()
            .unwrap()
            .into_iter()
            .find(|rule| rule.process_name == process)
            .unwrap();
        rule.manual_shortcut_allowed = true;
        rule.prose_context_allowed = true;
        database.app_rules().upsert(&rule).unwrap();
        for engine in [
            crate::settings::CorrectionEngine::Local,
            crate::settings::CorrectionEngine::Api,
        ] {
            let mut config = AppConfig::default();
            config.correction.engine = engine;
            let mut restricted_target = target();
            restricted_target.process_name = process.into();
            let manager = manager(&config, "This is user_name.");
            let mut request = request(&manager, &config);
            request.selected_text = true;
            let mut pipeline =
                CorrectionPipeline::start(|_| panic!("unsafe text reached correction engine"))
                    .unwrap();
            pipeline.database_path = Some(path.clone());
            assert!(!pipeline.submit(
                request,
                manager.active().unwrap(),
                restricted_target,
                STAMP,
                &config,
                vec![]
            ));
            assert!(pipeline.active.is_empty());
            assert!(pipeline.mailbox.0.lock().unwrap().jobs.is_empty());
            assert_eq!(
                manager.active().unwrap().editable_context(),
                "This is user_name."
            );
        }
    }
    drop(database);
    std::fs::remove_file(path).unwrap();
}

/// Unavailable exclusion storage refuses submission without dispatching work or changing session text.
#[test]
fn unavailable_exclusions_refuse_submission_without_storage_side_effects() {
    for failure in ["missing", "schema", "locked"] {
        let path = std::env::temp_dir().join(format!(
            "autofix-submit-policy-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = AppConfig::default();
        let manager = manager(&config, "teh");
        let mut connection = None;
        let mut database = None;
        if failure != "missing" {
            let writer = rusqlite::Connection::open(&path).unwrap();
            if failure == "schema" {
                writer
                    .execute_batch("create table sentinel (value text)")
                    .unwrap();
            } else {
                database = Some(crate::storage::Database::open(&path).unwrap());
                writer
                    .execute_batch("PRAGMA journal_mode=delete; BEGIN EXCLUSIVE")
                    .unwrap();
            }
            connection = Some(writer);
        }
        let mut pipeline =
            CorrectionPipeline::start(|_| panic!("unavailable exclusions must not reach engine"))
                .unwrap();
        pipeline.database_path = Some(path.clone());
        let started = Instant::now();
        assert!(!pipeline.submit(
            request(&manager, &config),
            manager.active().unwrap(),
            target(),
            STAMP,
            &config,
            vec![]
        ));
        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(pipeline.active.is_empty());
        assert!(pipeline.mailbox.0.lock().unwrap().jobs.is_empty());
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
        if failure == "schema" {
            let tables: usize = connection
                .as_ref()
                .unwrap()
                .query_row(
                    "select count(*) from sqlite_master where type = 'table'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(tables, 1);
        }
        drop(pipeline);
        drop(connection);
        drop(database);
        if failure == "missing" {
            assert!(!path.exists());
        } else {
            std::fs::remove_file(path).unwrap();
        }
    }
}

/// Submission reads compatible exclusion tables without invoking migrations, even during an API send.
#[test]
fn submit_uses_exclusion_snapshot_without_migration_writes() {
    for journal in ["delete", "wal"] {
        let path = std::env::temp_dir().join(format!(
            "autofix-submit-no-migrate-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let database = crate::storage::Database::open(&path).unwrap();
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer
            .execute_batch(&format!(
                "PRAGMA journal_mode={journal}; drop table schema_migrations;
            insert into custom_dictionary_entries(language_code,entry) values ('en','teh');"
            ))
            .unwrap();
        let config = AppConfig::default();
        let manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::start(|job| {
            assert_eq!(job.exclusions.terms, ["teh"]);
            CorrectionOutput::unchanged(
                job.input.executable_context.clone(),
                ConfidenceTier::High,
                NoChangeReason::NoCorrectionNeeded,
                0,
            )
        })
        .unwrap();
        pipeline.database_path = Some(path.clone());
        let guard = crate::storage::AppPolicyGuard::acquire(&path).unwrap();
        let started = Instant::now();
        assert!(pipeline.submit(
            request(&manager, &config),
            manager.active().unwrap(),
            target(),
            STAMP,
            &config,
            vec![]
        ));
        assert!(started.elapsed() < Duration::from_millis(250));
        wait_completion(&pipeline);
        let migration_tables: usize = writer
            .query_row(
                "select count(*) from sqlite_master where name = 'schema_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migration_tables, 0);
        drop(guard);
        drop(pipeline);
        drop(writer);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}

/// Persisted pairs filter worker edits without losing the original correction language.
#[test]
fn persisted_pair_filters_worker_edits_and_learns_original_language() {
    let path = std::env::temp_dir().join(format!(
        "autofix-pair-pipeline-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = crate::storage::Database::open(&path).unwrap();
    let rejection = crate::dictionary::Rejection::from_undo(
        "teh",
        "the",
        Some("en".into()),
        "notepad.exe".into(),
    )
    .unwrap();
    let learning = crate::settings::LearningConfig {
        mode: crate::settings::LearningMode::Automatic,
        ..Default::default()
    };
    db.dictionary().remember(&rejection, &learning).unwrap();
    let mut config = AppConfig::default();
    config.correction.preferred_language = Some("en".into());
    let mut manager = manager(&config, "teh wierd");
    let mut pipeline = CorrectionPipeline::with_database(&db).unwrap();
    let mut request = request(&manager, &config);
    request.language_info = crate::correction::LanguageInfo {
        primary_language: Some("en".into()),
        detected_languages: vec!["en".into()],
    };
    assert!(pipeline.submit(
        request,
        manager.active().unwrap(),
        target(),
        STAMP,
        &config,
        vec![]
    ));
    wait_completion(&pipeline);
    let replaced = Cell::new(0);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &replaced,
        true
    ));
    assert_eq!(replaced.get(), 1);
    assert_eq!(manager.active().unwrap().informative_context(), "teh weird");
    assert_eq!(
        manager
            .active()
            .unwrap()
            .undo_target()
            .unwrap()
            .language
            .as_deref(),
        Some("en")
    );
    assert!(manager
        .active_mut()
        .unwrap()
        .undo_last_correction(&config.context));
    assert_eq!(manager.active().unwrap().informative_context(), "teh wierd");
    drop(pipeline);
    drop(db);
    std::fs::remove_file(path).unwrap();
}

/// A real queued API job must not transmit after revocation commits behind an earlier send.
#[test]
fn queued_api_job_is_denied_after_rule_revocation_and_releases_its_slot() {
    use crate::storage::{AppRule, Database};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{SystemTime, UNIX_EPOCH},
    };

    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let profile = format!("queue-policy-{}-{id}", std::process::id());
    if let Err(error) = crate::secrets::set_secret(&profile, "test-key") {
        eprintln!("skipping provider test: credential store unavailable: {error}");
        return;
    }
    struct Cleanup {
        profile: String,
        path: PathBuf,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Err(error) = crate::secrets::delete_secret(&self.profile) {
                eprintln!("test credential cleanup failed: {error}");
            }
            if let Err(error) = std::fs::remove_file(&self.path) {
                eprintln!("test database cleanup failed: {error}");
            }
        }
    }
    let cleanup = Cleanup {
        profile: profile.clone(),
        path: std::env::temp_dir().join(format!(
            "autofix-queue-policy-{}-{id}.sqlite",
            std::process::id()
        )),
    };
    let database = Database::open(&cleanup.path).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sent, received) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let server = thread::spawn(move || {
        let started = Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        started.elapsed() < Duration::from_secs(3),
                        "missing first API send"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        // Windows sockets accepted from a nonblocking listener can inherit that mode.
        // Wait for the complete HTTP request before signalling the queued-job test.
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&buffer[..count]);
            if let Some(at) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..at]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|length| length.trim().parse().ok())
                    })
                    .unwrap();
                if request.len() >= at + 4 + length {
                    break;
                }
            }
        }
        sent.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        let body = serde_json::json!({"choices": [{"message": {"content":
            "{\"corrected_executable_text\":\"First.\",\"edits\":[]}"}}]})
        .to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        listener
    });
    let mut config = AppConfig::default();
    config.correction.engine = crate::settings::CorrectionEngine::Api;
    config.api.provider_preset = profile;
    config.api.base_url = Some(format!("http://127.0.0.1:{port}/v1"));
    config.api.timeout_auto_ms = 2_000;
    config.context.pending_queue_size = 2;
    let mut manager = manager(&config, "First.");
    let mut pipeline = CorrectionPipeline::with_database(&database).unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    received.recv_timeout(Duration::from_secs(3)).unwrap();
    manager.input(TypedInput::Text(" Second.".into()));
    submit_frozen(&mut pipeline, &mut manager, &config);
    manager.input(TypedInput::Text(" next".into()));
    assert_eq!(pipeline.mailbox.0.lock().unwrap().jobs.len(), 1);
    // Release the first send. Completion backpressure keeps the second job queued
    // while the rule writer waits for the transport's send guard to drop.
    release.send(()).unwrap();
    database
        .app_rules()
        .upsert(&AppRule {
            process_name: "notepad.exe".into(),
            window_title_pattern: None,
            list_behavior: "allowlist".into(),
            manual_shortcut_allowed: true,
            word_count_trigger_allowed: true,
            character_trigger_allowed: true,
            local_engine_allowed: true,
            api_engine_allowed: false,
            safety_mode: "auto".into(),
            prose_context_allowed: false,
        })
        .unwrap();
    wait_completion(&pipeline);
    assert!(!pipeline.finish(
        &mut manager,
        &config.context,
        || STAMP,
        |_| None,
        |_, _, _| panic!("revoked target must not be captured"),
        |_, _, _| -> bool { panic!("revoked target must not be edited") }
    ));
    // The rejected first segment restores the dependent queued request too.
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(pipeline.active.is_empty());
    assert_eq!(manager.active().unwrap().informative_context(), "");
    assert_eq!(
        manager.active().unwrap().editable_context(),
        "First. Second. next"
    );
    assert!(!pipeline.take_timeout_notice());
}

/// Only eligible manual API timeouts consume enabled timeout feedback.
#[test]
fn timeout_notice_is_manual_api_only_and_obeys_feedback_setting() {
    for trigger in [
        TriggerKind::ManualShortcut,
        TriggerKind::WordCount,
        TriggerKind::Character,
    ] {
        for engine in [EngineKind::CustomApi, EngineKind::LocalRule] {
            for enabled in [false, true] {
                let mut config = AppConfig::default();
                config.feedback.show_timeout_notice = enabled;
                let mut manager = manager(&config, "teh");
                let mut pipeline = CorrectionPipeline::start(|job| {
                    CorrectionOutput::timed_out(job.input.executable_context.clone(), 700)
                })
                .unwrap();
                let mut request = request(&manager, &config);
                request.trigger = trigger;
                request.engine = engine;
                pipeline.submit(
                    request,
                    manager.active().unwrap(),
                    target(),
                    STAMP,
                    &config,
                    Vec::new(),
                );
                wait_completion(&pipeline);
                let calls = Cell::new(0);
                assert!(!finish(
                    &mut pipeline,
                    &mut manager,
                    &config,
                    STAMP,
                    &calls,
                    true
                ));
                assert_eq!(
                    pipeline.take_timeout_notice(),
                    enabled
                        && trigger == TriggerKind::ManualShortcut
                        && engine == EngineKind::CustomApi
                );
                assert!(!pipeline.take_timeout_notice());
                assert_eq!(calls.get(), 0);
                assert_eq!(manager.active().unwrap().editable_context(), "teh");
            }
        }
    }
}

/// Stale input, cancellation, and secure targets suppress timeout feedback.
#[test]
fn stale_cancelled_or_secure_timeouts_cannot_show_notices() {
    for invalidation in 0..5 {
        let config = AppConfig::default();
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::start(|job| {
            CorrectionOutput::timed_out(job.input.executable_context.clone(), 700)
        })
        .unwrap();
        let mut request = request(&manager, &config);
        request.engine = EngineKind::CustomApi;
        pipeline.submit(
            request,
            manager.active().unwrap(),
            target(),
            STAMP,
            &config,
            Vec::new(),
        );
        wait_completion(&pipeline);
        match invalidation {
            0 => pipeline.cancel(),
            1 => {
                manager.input(TypedInput::Text(" next".into()));
            }
            2 => {
                manager.input(TypedInput::Uncertain(MovementSignal::FocusChange));
            }
            _ => {}
        }
        let current = Cell::new(STAMP);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || current.get(),
            |_| {
                let mut live = target();
                if invalidation == 3 {
                    live.is_password_or_protected = true;
                }
                if invalidation == 4 {
                    current.set(InputStamp {
                        sequence: STAMP.sequence + 1,
                        ..STAMP
                    });
                }
                Some(live)
            },
            |_, _, _| panic!("timeout must not read target text"),
            |_, _, _| -> bool { panic!("timeout must not replace text") },
        ));
        assert!(!pipeline.take_timeout_notice());
    }
}

/// Automatic timeout frees queue capacity without inspecting or modifying the live target.
#[test]
fn automatic_timeout_releases_frozen_slot_without_live_target_calls() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let mut pipeline = CorrectionPipeline::start(|job| {
        CorrectionOutput::timed_out(job.input.executable_context.clone(), 700)
    })
    .unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    wait_completion(&pipeline);
    manager.input(TypedInput::Text(" next".into()));
    assert!(!pipeline.finish(
        &mut manager,
        &config.context,
        || STAMP,
        |_| panic!("automatic timeout must skip live security/UIA calls"),
        |_, _, _| panic!("automatic timeout must not capture text"),
        |_, _, _| -> bool { panic!("automatic timeout must not replace text") },
    ));
    assert!(!pipeline.take_timeout_notice());
    assert!(pipeline.active.is_empty());
    assert_eq!(manager.active().unwrap().informative_context(), "");
    assert_eq!(manager.active().unwrap().editable_context(), "teh next");
}

/// Build a safe ordinary control with stable focus identity.
fn target() -> FocusedTarget {
    FocusedTarget {
        process_id: 1,
        process_name: "notepad.exe".into(),
        window_handle: 1,
        window_title: "Notes".into(),
        focused_element_id: Some(FocusedElementId::RuntimeId("editor".into())),
        is_elevated: false,
        is_password_or_protected: false,
        is_hidden_or_unavailable: false,
        field_safety_known: true,
        is_secure_desktop: false,
        is_lock_screen: false,
        is_credential_dialog: false,
    }
}

/// Build a session containing only known text typed during this test.
fn manager(config: &AppConfig, text: &str) -> SessionManager {
    let mut manager = SessionManager::new(config.context.clone());
    manager.focus(&target());
    manager.input(TypedInput::Text(text.into()));
    manager
}

/// Build a manual request from the active session snapshot.
fn request(manager: &SessionManager, config: &AppConfig) -> CorrectionRequest {
    let session = manager.active().unwrap();
    let mut request = triggers::manual(
        session.id(),
        session.informative_context(),
        &session.editable_context(),
        session.versions(),
        &SelectionCapture::NoSelection,
        config,
    )
    .unwrap();
    request.language_info = crate::correction::LanguageInfo {
        primary_language: Some("en".into()),
        detected_languages: vec!["en".into()],
    };
    request
}

/// Admit a manual snapshot against the stable test input stamp.
fn submit(pipeline: &mut CorrectionPipeline, manager: &SessionManager, config: &AppConfig) {
    pipeline.submit(
        request(manager, config),
        manager.active().unwrap(),
        target(),
        STAMP,
        config,
        Vec::new(),
    );
}

/// Freeze and admit an automatic range with its session-owned segment identity.
fn submit_frozen(
    pipeline: &mut CorrectionPipeline,
    manager: &mut SessionManager,
    config: &AppConfig,
) -> u64 {
    let mut request = request(manager, config);
    request.trigger = TriggerKind::WordCount;
    request.informative_context = manager.active().unwrap().correction_informative_context();
    let (segment, cancelled) = manager
        .active_mut()
        .unwrap()
        .freeze_pending(&config.context);
    for id in cancelled {
        pipeline.cancel_segment(id);
    }
    let id = segment.unwrap();
    (request.informative_context, request.executable_context) =
        manager.active().unwrap().pending_context(id).unwrap();
    request.pending_segment_id = Some(id);
    pipeline.submit(
        request,
        manager.active().unwrap(),
        target(),
        STAMP,
        config,
        Vec::new(),
    );
    id
}

/// FIFO results survive continued typing and preserve the active suffix.
#[test]
fn delayed_frozen_corrections_keep_all_results_and_preserve_newer_typing() {
    let mut config = AppConfig::default();
    config.context.pending_queue_size = 2;
    let mut manager = manager(&config, "teh");
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut pipeline = CorrectionPipeline::start(move |job| {
        started_tx.send(job.id).unwrap();
        release_rx.recv().unwrap();
        CorrectionEngines::default().correct_with(job.request.engine, &job.input)
    })
    .unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    manager.input(TypedInput::Text(" teh".into()));
    submit_frozen(&mut pipeline, &mut manager, &config);
    manager.input(TypedInput::Text(" 尾".into()));
    let stamp = InputStamp {
        sequence: STAMP.sequence + 5,
        ..STAMP
    };
    pipeline.invalidate(&mut manager, stamp);
    assert_eq!(pipeline.active.len(), 2);
    release_tx.send(()).unwrap();
    wait_completion(&pipeline);
    let expected_tail = " teh 尾";
    let live = manager.active().unwrap().executable_context();
    assert!(pipeline.finish(
        &mut manager,
        &config.context,
        || stamp,
        |_| Some(target()),
        |_, _, _| Some(live),
        |_, request, _| {
            assert_eq!(request.replacement_following_text, expected_tail);
            true
        }
    ));
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    release_tx.send(()).unwrap();
    wait_completion(&pipeline);
    let live = format!(
        "{}{}",
        manager.active().unwrap().informative_context(),
        manager.active().unwrap().executable_context()
    );
    assert!(pipeline.finish(
        &mut manager,
        &config.context,
        || stamp,
        |_| Some(target()),
        |_, _, _| Some(live),
        |_, request, _| {
            assert_eq!(request.executable_context, " teh");
            assert_eq!(request.replacement_following_text, " 尾");
            true
        }
    ));
    assert_eq!(manager.active().unwrap().informative_context(), "the the");
    assert_eq!(manager.active().unwrap().editable_context(), " 尾");
    assert!(pipeline.active.is_empty());
}

/// Cancelled output cannot publish over a newly admitted frozen segment.
#[test]
fn cancel_oldest_suppresses_late_transport_and_runs_new_segment() {
    let mut config = AppConfig::default();
    config.context.pending_queue_full_behavior =
        crate::settings::PendingQueueFullBehavior::CancelOldest;
    let mut manager = manager(&config, "teh");
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut pipeline = CorrectionPipeline::start(move |job| {
        started_tx.send(job.id).unwrap();
        release_rx.recv().unwrap();
        CorrectionEngines::default().correct_with(job.request.engine, &job.input)
    })
    .unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    let first = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let cancelled = pipeline.active.front().unwrap().cancelled.clone();
    manager.input(TypedInput::Text(" teh".into()));
    submit_frozen(&mut pipeline, &mut manager, &config);
    assert!(cancelled.load(Ordering::Acquire));
    assert_eq!(pipeline.active.len(), 1);
    assert_eq!(manager.active().unwrap().informative_context(), "");
    assert_eq!(
        pipeline.active.front().unwrap().request.executable_context,
        "teh teh"
    );
    release_tx.send(()).unwrap();
    let newest = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_ne!(first, newest);
    assert!(pipeline.mailbox.0.lock().unwrap().completion.is_none());
    release_tx.send(()).unwrap();
    wait_completion(&pipeline);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &Cell::new(0),
        true
    ));
    assert_eq!(manager.active().unwrap().informative_context(), "the the");
}

/// Failed and suppressed corrections restore unchecked text and release capacity.
#[test]
fn frozen_failures_release_capacity_without_committing_engine_output() {
    let config = AppConfig::default();
    for failure in 0..8 {
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit_frozen(&mut pipeline, &mut manager, &config);
        manager.input(TypedInput::Text(" next".into()));
        wait_completion(&pipeline);
        {
            let mut state = pipeline.mailbox.0.lock().unwrap();
            let output = &mut state.completion.as_mut().unwrap().output;
            match failure {
                0 => output.status = EngineStatus::TimedOut,
                1 => output.behavior = ConfidenceBehavior::Suggestion,
                2 => output.confidence = ConfidenceTier::Low,
                3 | 4 => {}
                5 => {
                    *output = CorrectionOutput::failed(
                        "teh".into(),
                        crate::correction::EngineFailure {
                            kind: crate::correction::EngineFailureKind::Internal,
                            message: "test failure".into(),
                            retryable: false,
                        },
                        0,
                    )
                }
                6 => output.behavior = ConfidenceBehavior::DoNothing,
                7 => {
                    *output = CorrectionOutput::unchanged(
                        "teh".into(),
                        ConfidenceTier::Low,
                        NoChangeReason::UnsupportedLanguage,
                        0,
                    )
                }
                _ => unreachable!(),
            }
        }
        let calls = Cell::new(0);
        let live = manager.active().unwrap().executable_context();
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || STAMP,
            |_| if failure == 4 { None } else { Some(target()) },
            |_, _, _| Some(live),
            |_, _, _| {
                calls.set(calls.get() + 1);
                false
            }
        ));
        assert_eq!(calls.get(), usize::from(failure == 3));
        assert_eq!(manager.active().unwrap().informative_context(), "");
        assert_eq!(manager.active().unwrap().editable_context(), "teh next");
        assert!(manager.active().unwrap().undo_target().is_none());
        assert!(manager
            .active_mut()
            .unwrap()
            .freeze_pending(&config.context)
            .0
            .is_some());
    }
}

/// Failure cancels dependent work and preserves the whole span for a successful retry.
#[test]
fn failed_frozen_prefix_restores_dependents_and_can_be_corrected_on_retry() {
    let mut config = AppConfig::default();
    config.context.pending_queue_size = 2;
    let mut manager = manager(&config, "teh");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    wait_completion(&pipeline);
    manager.input(TypedInput::Text(" wierd".into()));
    submit_frozen(&mut pipeline, &mut manager, &config);
    let cancelled = pipeline.active.back().unwrap().cancelled.clone();
    manager.input(TypedInput::Text(" 尾".into()));
    pipeline
        .mailbox
        .0
        .lock()
        .unwrap()
        .completion
        .as_mut()
        .unwrap()
        .output = CorrectionOutput::timed_out("teh".into(), 700);
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &Cell::new(0),
        false
    ));
    assert!(cancelled.load(Ordering::Acquire));
    assert!(pipeline.active.is_empty());
    assert_eq!(manager.active().unwrap().informative_context(), "");
    assert_eq!(manager.active().unwrap().editable_context(), "teh wierd 尾");
    submit_frozen(&mut pipeline, &mut manager, &config);
    wait_completion(&pipeline);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &Cell::new(0),
        true
    ));
    assert_eq!(
        manager.active().unwrap().informative_context(),
        "the weird 尾"
    );
    assert_eq!(manager.active().unwrap().editable_context(), "");
    assert!(manager
        .active_mut()
        .unwrap()
        .undo_last_correction(&config.context));
    assert_eq!(
        manager.active().unwrap().informative_context(),
        "teh wierd 尾"
    );
}

/// Every trigger accepts only checked unchanged text and shrinks its read-only context.
#[test]
fn unchanged_commit_policy_preserves_skips_for_all_triggers() {
    let path = std::env::temp_dir().join(format!(
        "autofix-no-change-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let database = crate::storage::Database::open(&path).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    for trigger in [
        TriggerKind::ManualShortcut,
        TriggerKind::WordCount,
        TriggerKind::Character,
        TriggerKind::FinalFixBeforeReanchor,
    ] {
        for reason in [
            NoChangeReason::NoCorrectionNeeded,
            NoChangeReason::AllCandidatesProtected,
            NoChangeReason::ConfidenceBelowConfiguredBehavior,
            NoChangeReason::UnsupportedLanguage,
            NoChangeReason::UncertainLanguage,
            NoChangeReason::EngineUnavailable,
            NoChangeReason::TimedOut,
            NoChangeReason::EngineError,
        ] {
            let mut config = AppConfig::default();
            config.context.informative_context_max_chars = 12;
            config.logging.debug_mode_enabled = true;
            config.logging.full_text_debug_mode_enabled = true;
            let mut manager = manager(&config, "hello");
            manager.set_informative_context("Old context ".into());
            database.clear_logs().unwrap();
            let mut pipeline = CorrectionPipeline::with_database(&database).unwrap();
            if matches!(trigger, TriggerKind::WordCount | TriggerKind::Character) {
                submit_frozen(&mut pipeline, &mut manager, &config);
                pipeline.active.front_mut().unwrap().request.trigger = trigger;
                manager.input(TypedInput::Text(" 尾".into()));
            } else {
                let mut request = request(&manager, &config);
                request.trigger = trigger;
                assert!(pipeline.submit(
                    request,
                    manager.active().unwrap(),
                    target(),
                    STAMP,
                    &config,
                    vec![]
                ));
            }
            wait_completion(&pipeline);
            pipeline
                .mailbox
                .0
                .lock()
                .unwrap()
                .completion
                .as_mut()
                .unwrap()
                .output = CorrectionOutput::unchanged(
                "hello".into(),
                ConfidenceTier::Low,
                reason.clone(),
                17,
            );
            let accepted =
                matches!(
                    reason,
                    NoChangeReason::NoCorrectionNeeded | NoChangeReason::AllCandidatesProtected
                ) || (matches!(trigger, TriggerKind::WordCount | TriggerKind::Character)
                    && reason == NoChangeReason::ConfidenceBelowConfiguredBehavior);
            let calls = Cell::new(0);
            assert_eq!(
                finish(&mut pipeline, &mut manager, &config, STAMP, &calls, false),
                accepted
            );
            assert_eq!(calls.get(), 0);
            // A completion is consumed once, including its metadata event.
            assert!(!finish(
                &mut pipeline,
                &mut manager,
                &config,
                STAMP,
                &calls,
                false
            ));
            let events: i64 = connection
                .query_row("select count(*) from correction_metadata", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(events, i64::from(accepted));
            if accepted {
                let metadata: (String, String, String, String, String, String, u64) = connection.query_row(
                    "select session_id, app_process_name, trigger_type, confidence_tier, replacement_method, result_reason, latency_ms from correction_metadata",
                    [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
                ).unwrap();
                assert_eq!(
                    metadata,
                    (
                        manager.active().unwrap().id().to_string(),
                        "notepad.exe".into(),
                        trigger.as_str().into(),
                        "low".into(),
                        "none".into(),
                        match reason {
                            NoChangeReason::NoCorrectionNeeded => "no_correction_needed",
                            NoChangeReason::ConfidenceBelowConfiguredBehavior => {
                                "confidence_below_configured_behavior"
                            }
                            _ => "all_candidates_protected",
                        }
                        .into(),
                        17
                    )
                );
            }
            let debug_events: i64 = connection
                .query_row("select count(*) from debug_events", [], |row| row.get(0))
                .unwrap();
            assert_eq!(debug_events, 0);
            let session = manager.active().unwrap();
            let newer = if matches!(trigger, TriggerKind::WordCount | TriggerKind::Character) {
                " 尾"
            } else {
                ""
            };
            assert_eq!(
                session.editable_context(),
                if accepted {
                    newer.to_owned()
                } else {
                    format!("hello{newer}")
                }
            );
            if accepted {
                assert!(session.informative_context().ends_with("hello"));
                assert!(session.informative_context().chars().count() <= 12);
            } else {
                assert_eq!(session.informative_context(), "Old context ");
            }
            assert!(session.undo_target().is_none());
        }
    }
    drop(connection);
    drop(database);
    std::fs::remove_file(path).unwrap();
}

/// Completion rejects position changes and input arriving during live security validation.
#[test]
fn frozen_result_revalidates_queued_position_and_input_during_security_check() {
    let config = AppConfig::default();
    for movement in [false, true] {
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit_frozen(&mut pipeline, &mut manager, &config);
        wait_completion(&pipeline);
        let stamp = Cell::new(STAMP);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || stamp.get(),
            |_| {
                stamp.set(InputStamp {
                    sequence: STAMP.sequence + 1,
                    position: STAMP.position + u64::from(movement),
                });
                Some(target())
            },
            |_, _, _| panic!("raced input reached live range validation"),
            |_, _, _| -> bool { panic!("raced input reached replacement") }
        ));
    }
}

/// Manual override cancels frozen publication and restores the complete typed scope.
#[test]
fn manual_override_cancels_frozen_completion_and_restores_full_scope() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    wait_completion(&pipeline);
    let cancelled = pipeline.active.front().unwrap().cancelled.clone();
    manager.input(TypedInput::Text(" next".into()));
    pipeline.cancel();
    manager.active_mut().unwrap().restore_pending();
    submit(&mut pipeline, &manager, &config);
    assert!(cancelled.load(Ordering::Acquire));
    assert_eq!(
        pipeline.active.front().unwrap().request.executable_context,
        "teh next"
    );
    wait_completion(&pipeline);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &Cell::new(0),
        true
    ));
    assert_eq!(manager.active().unwrap().informative_context(), "the next");
}

/// An unchanged frozen result retires only its original and preserves newer typing.
#[test]
fn unchanged_frozen_result_retires_only_its_segment_without_native_mutation() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "hello");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    manager.input(TypedInput::Text(" next".into()));
    wait_completion(&pipeline);
    let calls = Cell::new(0);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        false
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(manager.active().unwrap().informative_context(), "hello");
    assert_eq!(manager.active().unwrap().editable_context(), " next");
}

/// Hook input must enter the session before completion validates the live caret range.
#[test]
fn input_processor_defers_frozen_completion_until_hook_input_is_drained() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "hello");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    wait_completion(&pipeline);
    let mut processor = crate::background::InputProcessor {
        feedback: crate::background::feedback::Feedback::default(),
        learner: crate::dictionary::Learner::default(),
        pipeline,
        processed_input_sequence: crate::background::input_listener::current_input_sequence()
            .wrapping_sub(1),
        config,
        session_manager: manager,
        database: crate::storage::Database::open_memory().unwrap(),
    };
    processor.finish_correction();
    assert!(processor
        .pipeline
        .mailbox
        .0
        .lock()
        .unwrap()
        .completion
        .is_some());
    assert_eq!(processor.pipeline.active.len(), 1);
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .informative_context(),
        ""
    );
}

/// Wait for a worker completion with a bounded deadline to detect stalled tests.
fn wait_completion(pipeline: &CorrectionPipeline) {
    let (lock, ready) = &*pipeline.mailbox;
    let (state, _) = ready
        .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(2), |state| {
            state.completion.is_none()
        })
        .unwrap();
    assert!(
        state.completion.is_some(),
        "correction worker did not complete"
    );
}

/// Validate a stable live range and count replacement calls for the completion under test.
fn finish(
    pipeline: &mut CorrectionPipeline,
    manager: &mut SessionManager,
    config: &AppConfig,
    stamp: InputStamp,
    replaced: &Cell<usize>,
    success: bool,
) -> bool {
    let live = format!(
        "{}{}",
        manager.active().unwrap().informative_context(),
        manager.active().unwrap().executable_context()
    );
    pipeline.finish(
        manager,
        &config.context,
        || stamp,
        |_| Some(target()),
        |_, _, _| Some(live),
        |_, _, _| {
            replaced.set(replaced.get() + 1);
            success
        },
    )
}

/// A policy writer must not delay accepted no-change commits or later input.
#[test]
fn no_change_metadata_contention_never_stalls_completion() {
    for mode in ["delete", "wal"] {
        let path = std::env::temp_dir().join(format!(
            "autofix-nowait-{mode}-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let database = crate::storage::Database::open(&path).unwrap();
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer.pragma_update(None, "journal_mode", mode).unwrap();
        let config = AppConfig::default();
        let mut manager = manager(&config, "hello");
        let mut pipeline = CorrectionPipeline::with_database(&database).unwrap();
        submit_frozen(&mut pipeline, &mut manager, &config);
        wait_completion(&pipeline);
        manager.input(TypedInput::Text(" 尾".into()));
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let calls = Cell::new(0);
        let started = Instant::now();
        assert!(finish(
            &mut pipeline,
            &mut manager,
            &config,
            STAMP,
            &calls,
            false
        ));
        assert!(started.elapsed() < Duration::from_millis(250));
        assert_eq!(calls.get(), 0);
        assert_eq!(manager.active().unwrap().informative_context(), "hello");
        assert_eq!(manager.active().unwrap().editable_context(), " 尾");
        manager.input(TypedInput::Text(" next".into()));
        assert_eq!(manager.active().unwrap().editable_context(), " 尾 next");
        assert!(pipeline.active.is_empty());
        writer.execute_batch("ROLLBACK").unwrap();
        let count: i64 = writer
            .query_row("select count(*) from correction_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        // Losing optional telemetry does not poison later accepted commits.
        submit(&mut pipeline, &manager, &config);
        wait_completion(&pipeline);
        assert!(finish(
            &mut pipeline,
            &mut manager,
            &config,
            STAMP,
            &calls,
            false
        ));
        let count: i64 = writer
            .query_row("select count(*) from correction_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        drop(pipeline);
        drop(writer);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}

/// Manual expansion must fit the Unicode typed buffer before any native mutation.
#[test]
fn oversized_manual_result_is_refused_before_native_replacement() {
    let config = AppConfig::default();
    let original = format!("{}teh", "é".repeat(4093));
    let mut manager = manager(&config, &original);
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit(&mut pipeline, &manager, &config);
    wait_completion(&pipeline);
    let mut state = pipeline.mailbox.0.lock().unwrap();
    state.completion.as_mut().unwrap().output = CorrectionOutput::changed(
        format!("{}the!", "é".repeat(4093)),
        ConfidenceTier::High,
        None,
        1,
    );
    drop(state);
    let calls = Cell::new(0);
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(manager.active().unwrap().editable_context(), original);
    assert!(manager.active().unwrap().informative_context().is_empty());
    assert!(manager.active().unwrap().undo_target().is_none());
}

/// Input arriving after a successful edit invalidates ownership and dependent work.
#[test]
fn input_race_after_native_success_drops_session_instead_of_restoring_stale_text() {
    for frozen in [false, true] {
        let mut config = AppConfig::default();
        config.context.pending_queue_size = 2;
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        if frozen {
            submit_frozen(&mut pipeline, &mut manager, &config);
            manager.input(TypedInput::Text(" next".into()));
            submit_frozen(&mut pipeline, &mut manager, &config);
        } else {
            submit(&mut pipeline, &manager, &config);
        }
        wait_completion(&pipeline);
        let live = manager.active().unwrap().executable_context();
        let current = Cell::new(STAMP);
        let calls = Cell::new(0);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || current.get(),
            |_| Some(target()),
            |_, _, _| Some(live),
            |_, _, _| {
                calls.set(calls.get() + 1);
                current.set(InputStamp {
                    sequence: STAMP.sequence + 1,
                    ..STAMP
                });
                true
            }
        ));
        assert_eq!(calls.get(), 1);
        assert!(manager.active().is_none());
        assert!(pipeline.active.is_empty());
        assert!(pipeline.mailbox.0.lock().unwrap().jobs.is_empty());
        manager.focus(&target());
        manager.input(TypedInput::Text("fresh".into()));
        assert_eq!(manager.active().unwrap().editable_context(), "fresh");
        assert!(manager.active().unwrap().undo_target().is_none());
    }
}

/// Confirmed replacement commits once and records the original span for undo.
#[test]
fn valid_result_replaces_once_then_records_commit_and_undo() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit(&mut pipeline, &manager, &config);
    wait_completion(&pipeline);
    let calls = Cell::new(0);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(calls.get(), 1);
    assert_eq!(pipeline.take_feedback(), Some((Event::Applied, true)));
    assert_eq!(pipeline.take_feedback(), None);
    assert_eq!(manager.active().unwrap().informative_context(), "the");
    assert_eq!(manager.active().unwrap().editable_context(), "");
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    assert!(manager
        .active_mut()
        .unwrap()
        .undo_last_correction(&config.context));
    assert_eq!(manager.active().unwrap().informative_context(), "teh");
}

#[test]
fn manual_failure_feedback_is_validated_and_contains_no_provider_text() {
    for unsafe_target in [false, true] {
        let config = AppConfig::default();
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::start(|job| {
            CorrectionOutput::failed(
                job.input.executable_context.clone(),
                crate::correction::EngineFailure {
                    kind: crate::correction::EngineFailureKind::Internal,
                    message: "secret provider document text".into(),
                    retryable: false,
                },
                0,
            )
        })
        .unwrap();
        submit(&mut pipeline, &manager, &config);
        wait_completion(&pipeline);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || STAMP,
            |_| {
                let mut target = target();
                target.is_password_or_protected = unsafe_target;
                Some(target)
            },
            |_, _, _| panic!("failure must not read document text"),
            |_, _, _| -> bool { panic!("failure must not mutate") }
        ));
        assert_eq!(
            pipeline.take_feedback(),
            if unsafe_target {
                None
            } else {
                Some((Event::Error, true))
            }
        );
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
    }
}

#[test]
fn medium_previews_are_manual_opt_in_and_never_mutate_or_commit() {
    for enabled in [false, true] {
        for automatic in [false, true] {
            let mut config = AppConfig::default();
            config.feedback.show_medium_confidence_suggestions = enabled;
            let mut manager = manager(&config, "teh");
            let mut pipeline = CorrectionPipeline::start(|job| {
                let behavior = job.input.confidence_behavior.behavior_for(
                    ConfidenceTier::Medium,
                    job.input.trigger_type,
                    job.input.suggestion_ui_available,
                );
                if behavior == ConfidenceBehavior::Suggestion {
                    CorrectionOutput {
                        corrected_executable_text: "the".into(),
                        changes_needed: true,
                        confidence: ConfidenceTier::Medium,
                        behavior,
                        changes: None,
                        no_change_reason: None,
                        engine_latency_ms: 0,
                        status: EngineStatus::Completed,
                    }
                } else {
                    CorrectionOutput::unchanged(
                        job.input.executable_context.clone(),
                        ConfidenceTier::Medium,
                        NoChangeReason::ConfidenceBelowConfiguredBehavior,
                        0,
                    )
                }
            })
            .unwrap();
            if automatic {
                submit_frozen(&mut pipeline, &mut manager, &config);
            } else {
                submit(&mut pipeline, &manager, &config);
            }
            wait_completion(&pipeline);
            let calls = Cell::new(0);
            assert_eq!(
                finish(&mut pipeline, &mut manager, &config, STAMP, &calls, true),
                automatic
            );
            assert_eq!(calls.get(), 0);
            let preview = pipeline.take_suggestion();
            if let Some(preview) = &preview {
                assert_eq!(preview.stamp, STAMP);
                assert_eq!(preview.target, target());
                assert!(!preview.cancelled.load(Ordering::Acquire));
                pipeline.cancel();
                assert!(preview.cancelled.load(Ordering::Acquire));
            }
            assert_eq!(
                preview.map(|preview| preview.text),
                if enabled && !automatic && cfg!(windows) {
                    Some("AutoFix suggestion: the".into())
                } else {
                    None
                }
            );
            assert!(pipeline.take_suggestion().is_none());
            assert_eq!(
                manager.active().unwrap().editable_context(),
                if automatic { "" } else { "teh" }
            );
            assert_eq!(
                manager.active().unwrap().informative_context(),
                if automatic { "teh" } else { "" }
            );
            assert!(manager.active().unwrap().undo_target().is_none());
        }
    }
}

/// An engine cannot grant itself silent apply or preview permission.
#[test]
fn completion_behavior_must_match_admitted_confidence_policy() {
    for configured in [
        ConfidenceBehavior::DoNothing,
        ConfidenceBehavior::Suggestion,
        ConfidenceBehavior::Silent,
    ] {
        for enabled in [false, true] {
            for trigger in [
                TriggerKind::ManualShortcut,
                TriggerKind::WordCount,
                TriggerKind::Character,
                TriggerKind::FinalFixBeforeReanchor,
            ] {
                for confidence in [ConfidenceTier::Medium, ConfidenceTier::Low] {
                    for reported in [ConfidenceBehavior::Silent, ConfidenceBehavior::Suggestion] {
                        let mut config = AppConfig::default();
                        config.correction.medium_confidence_behavior = configured;
                        config.feedback.show_medium_confidence_suggestions = enabled;
                        let mut manager = manager(&config, "teh");
                        let mut pipeline = CorrectionPipeline::start(move |_| {
                            let mut output =
                                CorrectionOutput::changed("the".into(), confidence, None, 0);
                            output.behavior = reported;
                            output
                        })
                        .unwrap();
                        let mut request = request(&manager, &config);
                        request.trigger = trigger;
                        assert!(pipeline.submit(
                            request,
                            manager.active().unwrap(),
                            target(),
                            STAMP,
                            &config,
                            vec![],
                        ));
                        wait_completion(&pipeline);
                        let calls = Cell::new(0);
                        let silent = confidence == ConfidenceTier::Medium
                            && configured == ConfidenceBehavior::Silent
                            && reported == ConfidenceBehavior::Silent;
                        let preview = confidence == ConfidenceTier::Medium
                            && configured == ConfidenceBehavior::Suggestion
                            && reported == ConfidenceBehavior::Suggestion
                            && enabled
                            && cfg!(windows)
                            && trigger == TriggerKind::ManualShortcut;
                        assert_eq!(
                            finish(&mut pipeline, &mut manager, &config, STAMP, &calls, true,),
                            silent,
                            "{configured:?}, {trigger:?}, {confidence:?}, {reported:?}"
                        );
                        assert_eq!(calls.get(), usize::from(silent));
                        assert_eq!(pipeline.take_suggestion().is_some(), preview);
                        let session = manager.active().unwrap();
                        assert_eq!(session.undo_target().is_some(), silent);
                        if !silent {
                            assert_eq!(session.editable_context(), "teh");
                            assert!(session.informative_context().is_empty());
                        }
                    }
                }
            }
        }
    }
}

/// Real local medium corrections use the saved policy for manual and frozen work.
#[test]
fn medium_local_results_skip_preview_or_apply_without_losing_typed_text() {
    for configured in [
        ConfidenceBehavior::DoNothing,
        ConfidenceBehavior::Suggestion,
        ConfidenceBehavior::Silent,
    ] {
        for automatic in [false, true] {
            let mut config = AppConfig::default();
            config.correction.medium_confidence_behavior = configured;
            let mut manager = manager(&config, "accomodate");
            let mut pipeline = CorrectionPipeline::new().unwrap();
            if automatic {
                submit_frozen(&mut pipeline, &mut manager, &config);
            } else {
                submit(&mut pipeline, &manager, &config);
            }
            wait_completion(&pipeline);
            let calls = Cell::new(0);
            let silent = configured == ConfidenceBehavior::Silent;
            let committed = silent || automatic;
            assert_eq!(
                finish(&mut pipeline, &mut manager, &config, STAMP, &calls, true,),
                committed
            );
            assert_eq!(calls.get(), usize::from(silent));
            assert!(pipeline.take_suggestion().is_none());
            let session = manager.active().unwrap();
            assert_eq!(session.undo_target().is_some(), silent);
            assert_eq!(
                session.informative_context(),
                if silent {
                    "accommodate"
                } else if automatic {
                    "accomodate"
                } else {
                    ""
                }
            );
            assert_eq!(
                session.editable_context(),
                if committed { "" } else { "accomodate" }
            );
        }
    }
}

#[test]
fn unsafe_or_stale_suggestions_cannot_surface_a_preview() {
    for failure in 0..5 {
        let mut config = AppConfig::default();
        config.feedback.show_medium_confidence_suggestions = true;
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit(&mut pipeline, &manager, &config);
        wait_completion(&pipeline);
        {
            let mut state = pipeline.mailbox.0.lock().unwrap();
            let output = &mut state.completion.as_mut().unwrap().output;
            output.confidence = ConfidenceTier::Medium;
            output.behavior = ConfidenceBehavior::Suggestion;
        }
        if failure == 0 {
            pipeline.cancel();
        }
        if failure == 1 {
            manager.input(TypedInput::Text(" new".into()));
        }
        let stamp = Cell::new(STAMP);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || stamp.get(),
            |_| {
                let mut t = target();
                t.is_password_or_protected = failure == 2;
                Some(t)
            },
            |_, _, _| {
                if failure == 3 {
                    return None;
                }
                if failure == 4 {
                    stamp.set(InputStamp {
                        sequence: STAMP.sequence + 1,
                        ..STAMP
                    });
                }
                Some("teh".into())
            },
            |_, _, _| -> bool { panic!("suggestion must never mutate") }
        ));
        assert!(pipeline.take_suggestion().is_none());
    }
}

/// Invalidated snapshots cannot cross context, caret, or session boundaries.
#[test]
fn changed_context_movement_commit_and_recreated_session_discard_results() {
    let config = AppConfig::default();
    for change in 0..7 {
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit(&mut pipeline, &manager, &config);
        wait_completion(&pipeline);
        let mut stamp = STAMP;
        match change {
            0 => {
                manager.input(TypedInput::Text("!".into()));
            }
            1 => {
                manager.input(TypedInput::Left);
            }
            2 => manager
                .active_mut()
                .unwrap()
                .complete_without_changes(&config.context),
            3 => {
                let old_id = manager.active().unwrap().id();
                let versions = manager.active().unwrap().versions();
                manager.deactivate(MovementSignal::FocusChange);
                manager.focus(&target());
                manager.input(TypedInput::Text("teh".into()));
                assert_ne!(old_id, manager.active().unwrap().id());
                assert_eq!(versions, manager.active().unwrap().versions());
            }
            4 => {
                stamp.sequence += 1;
            } // Typing still queued for processing.
            5 => {
                stamp.position += 1;
            } // Focus/mouse event still queued.
            6 => manager.set_informative_context("changed ".into()),
            _ => unreachable!(),
        }
        let calls = Cell::new(0);
        assert!(!finish(
            &mut pipeline,
            &mut manager,
            &config,
            stamp,
            &calls,
            true
        ));
        assert_eq!(calls.get(), 0, "stale case {change} reached replacement");
    }
}

/// Security and hook races during completion prevent mutation.
#[test]
fn security_and_input_changes_during_live_validation_block_replacement() {
    let config = AppConfig::default();
    for change in 0..6 {
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit(&mut pipeline, &manager, &config);
        wait_completion(&pipeline);
        let stamp = Cell::new(STAMP);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || stamp.get(),
            |_| {
                let mut target = target();
                match change {
                    0 => target.is_password_or_protected = true,
                    1 => target.window_handle = 2,
                    2 => {
                        stamp.set(InputStamp {
                            sequence: STAMP.sequence + 1,
                            ..STAMP
                        });
                    }
                    3 => return None,
                    4 => {
                        target.focused_element_id =
                            Some(FocusedElementId::RuntimeId("other".into()))
                    }
                    5 => target.focused_element_id = None,
                    _ => unreachable!(),
                }
                Some(target)
            },
            |_, _, _| panic!("invalid result reached live range validation"),
            |_, _, _| -> bool { panic!("invalid result reached replacement") }
        ));
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
    }
}

/// Text elsewhere in a control cannot substitute for an exact pre-caret range.
#[test]
fn live_range_must_match_exactly_before_caret() {
    let config = AppConfig::default();
    for live in [
        None,
        Some("th e"),
        Some("teh extra"),
        Some("the"),
        Some("teh teh extra"),
    ] {
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit_frozen(&mut pipeline, &mut manager, &config);
        wait_completion(&pipeline);
        let calls = Cell::new(0);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || STAMP,
            |_| Some(target()),
            |_, _, _| live.map(str::to_owned),
            |_, _, _| {
                calls.set(calls.get() + 1);
                true
            }
        ));
        assert_eq!(calls.get(), 0);
        assert_eq!(manager.active().unwrap().informative_context(), "");
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
    }
}

/// Frozen replacement scope excludes and preserves all known following typed text.
#[test]
fn frozen_range_can_end_before_caret_but_never_include_following_text() {
    assert!(exact_range_before_caret(
        "prefix teh next",
        "teh next",
        "teh",
        " next"
    ));
    assert!(!exact_range_before_caret(
        "prefix teh next",
        "teh next",
        "teh next",
        " next"
    ));
    assert!(!exact_range_before_caret(
        "prefix the next",
        "teh next",
        "teh",
        " next"
    ));
    assert!(!exact_range_before_caret(
        "prefix teh next!",
        "teh next",
        "teh",
        " next"
    ));
}

/// Only completed silent results above low confidence can reach replacement.
#[test]
fn failures_suggestions_and_low_confidence_never_commit_or_replace() {
    let config = AppConfig::default();
    for failure in 0..6 {
        let mut manager = manager(&config, "teh");
        let mut pipeline = CorrectionPipeline::new().unwrap();
        submit(&mut pipeline, &manager, &config);
        wait_completion(&pipeline);
        {
            let mut state = pipeline.mailbox.0.lock().unwrap();
            let output = &mut state.completion.as_mut().unwrap().output;
            match failure {
                0 => *output = CorrectionOutput::timed_out("teh".into(), 700),
                1 => output.behavior = ConfidenceBehavior::Suggestion,
                2 => output.behavior = ConfidenceBehavior::DoNothing,
                3 => output.confidence = ConfidenceTier::Low,
                4 => {
                    // A suppressed edit is not successful unchanged work.
                    *output = CorrectionOutput::unchanged(
                        "teh".into(),
                        ConfidenceTier::Medium,
                        NoChangeReason::ConfidenceBelowConfiguredBehavior,
                        0,
                    );
                }
                5 => {} // Replacement itself fails.
                _ => unreachable!(),
            }
        }
        let calls = Cell::new(0);
        assert!(!finish(
            &mut pipeline,
            &mut manager,
            &config,
            STAMP,
            &calls,
            false
        ));
        assert_eq!(calls.get(), usize::from(failure == 5));
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
        assert!(!manager
            .active_mut()
            .unwrap()
            .undo_last_correction(&config.context));
    }
}

/// Unchanged completion still requires a valid session and exact live caret range.
#[test]
fn unchanged_success_commits_only_after_validation() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "hello");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit(&mut pipeline, &manager, &config);
    wait_completion(&pipeline);
    let live = manager.active().unwrap().executable_context();
    assert!(pipeline.finish(
        &mut manager,
        &config.context,
        || STAMP,
        |_| Some(target()),
        |_, _, _| Some(live),
        |_, _, _| -> bool { panic!("unchanged text needs no replacement") }
    ));
    assert_eq!(manager.active().unwrap().informative_context(), "hello");
    assert_eq!(manager.active().unwrap().editable_context(), "");
}

/// Completion ownership rejects superseded, out-of-order, and duplicate results.
#[test]
fn superseded_reordered_and_duplicate_results_cannot_replace() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit(&mut pipeline, &manager, &config);
    wait_completion(&pipeline);
    let old = pipeline
        .mailbox
        .0
        .lock()
        .unwrap()
        .completion
        .take()
        .unwrap();
    let cancelled = Arc::clone(&pipeline.active.front().unwrap().cancelled);
    submit(&mut pipeline, &manager, &config);
    assert!(cancelled.load(Ordering::Acquire));
    wait_completion(&pipeline);
    let latest = pipeline
        .mailbox
        .0
        .lock()
        .unwrap()
        .completion
        .take()
        .unwrap();
    pipeline.mailbox.0.lock().unwrap().completion = Some(old);
    let calls = Cell::new(0);
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    let duplicate = Completion {
        id: latest.id,
        output: latest.output.clone(),
    };
    pipeline.mailbox.0.lock().unwrap().completion = Some(latest);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    pipeline.mailbox.0.lock().unwrap().completion = Some(duplicate);
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(calls.get(), 1);
}

/// Slow correction execution leaves automatic input processing independent.
#[test]
fn slow_engine_never_blocks_automatic_input_and_keeps_only_latest_queued_work() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut pipeline = CorrectionPipeline::start(move |job| {
        started_tx.send(job.id).unwrap();
        release_rx.recv().unwrap();
        CorrectionEngines::default().correct_with(job.request.engine, &job.input)
    })
    .unwrap();
    let mut automatic = request(&manager, &config);
    automatic.trigger = TriggerKind::WordCount;
    pipeline.submit(
        automatic,
        manager.active().unwrap(),
        target(),
        STAMP,
        &config,
        Vec::new(),
    );
    let first = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let cancelled = Arc::clone(&pipeline.active.front().unwrap().cancelled);
    let start = Instant::now();
    manager.input(TypedInput::Text("!".into()));
    pipeline.invalidate(&mut manager, STAMP);
    assert!(cancelled.load(Ordering::Acquire));
    manager.input(TypedInput::Backspace);
    for _ in 0..32 {
        submit(&mut pipeline, &manager, &config);
    }
    assert!(start.elapsed() < Duration::from_secs(1));
    let latest = pipeline.active.front().unwrap().id;
    assert_ne!(first, latest);
    // A manual wait is bounded even while the transport cannot be interrupted.
    let start = Instant::now();
    pipeline.wait_manual();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(pipeline.mailbox.0.lock().unwrap().completion.is_none());
    release_tx.send(()).unwrap();
    assert_eq!(
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        latest
    );
    release_tx.send(()).unwrap();
    wait_completion(&pipeline);
    assert_eq!(
        pipeline
            .mailbox
            .0
            .lock()
            .unwrap()
            .completion
            .as_ref()
            .unwrap()
            .id,
        latest
    );
    assert!(started_rx.try_recv().is_err());
    let calls = Cell::new(0);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
}

/// Changed selected-text results cannot mutate without proving the live caret endpoint.
#[test]
fn selected_text_result_is_discarded_without_caret_end_proof() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh later");
    let session = manager.active().unwrap();
    let selected = SelectionCapture::Selected {
        text: "teh".into(),
        preceding: String::new(),
        following: " later foreign".into(),
        executable_prefix: Some(String::new()),
    };
    let mut request = triggers::manual(
        session.id(),
        "",
        &session.editable_context(),
        session.versions(),
        &selected,
        &config,
    )
    .unwrap();
    request.language_info = crate::correction::LanguageInfo {
        primary_language: Some("en".into()),
        detected_languages: vec!["en".into()],
    };
    let mut pipeline = CorrectionPipeline::new().unwrap();
    pipeline.submit(request, session, target(), STAMP, &config, Vec::new());
    wait_completion(&pipeline);
    let calls = Cell::new(0);
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(manager.active().unwrap().informative_context(), "");
    assert_eq!(manager.active().unwrap().editable_context(), "teh later");
    assert!(!manager
        .active_mut()
        .unwrap()
        .undo_last_correction(&config.context));
}

/// Even unchanged selected-text results cannot retire an unproven caret range.
#[test]
fn unchanged_selection_is_discarded_without_caret_end_proof() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "hello later");
    let session = manager.active().unwrap();
    let selected = SelectionCapture::Selected {
        text: "hello".into(),
        preceding: String::new(),
        following: " later".into(),
        executable_prefix: Some(String::new()),
    };
    let mut request = triggers::manual(
        session.id(),
        "",
        &session.editable_context(),
        session.versions(),
        &selected,
        &config,
    )
    .unwrap();
    request.language_info = crate::correction::LanguageInfo {
        primary_language: Some("en".into()),
        detected_languages: vec!["en".into()],
    };
    let mut pipeline = CorrectionPipeline::new().unwrap();
    pipeline.submit(request, session, target(), STAMP, &config, Vec::new());
    wait_completion(&pipeline);
    let calls = Cell::new(0);
    assert!(!finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        false
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(manager.active().unwrap().editable_context(), "hello later");
    assert_eq!(manager.active().unwrap().informative_context(), "");
}
