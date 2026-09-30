use super::*;
use crate::background::{
    context_capture::SelectionCapture,
    triggers,
    typing::{MovementSignal, TypedInput},
};
use std::{cell::Cell, sync::mpsc, time::Instant};

const STAMP: InputStamp = InputStamp {
    position: 7,
    sequence: 12,
};

fn target() -> FocusedTarget {
    FocusedTarget {
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

fn manager(config: &AppConfig, text: &str) -> SessionManager {
    let mut manager = SessionManager::new(config.context.clone());
    manager.focus(&target());
    manager.input(TypedInput::Text(text.into()));
    manager
}

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
    if let Some(id) = cancelled {
        pipeline.cancel_segment(id);
    }
    let id = segment.unwrap();
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
    pipeline.invalidate(&manager, stamp);
    assert_eq!(pipeline.active.len(), 2);
    release_tx.send(()).unwrap();
    wait_completion(&pipeline);
    let expected_tail = " teh 尾";
    assert!(pipeline.finish(
        &mut manager,
        &config.context,
        || stamp,
        |_| Some(target()),
        |_, request, _| {
            assert_eq!(request.replacement_following_text, expected_tail);
            true
        }
    ));
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    release_tx.send(()).unwrap();
    wait_completion(&pipeline);
    assert!(pipeline.finish(
        &mut manager,
        &config.context,
        || stamp,
        |_| Some(target()),
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
    assert_eq!(manager.active().unwrap().informative_context(), "teh");
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
    assert_eq!(manager.active().unwrap().informative_context(), "teh the");
}

#[test]
fn frozen_failures_release_capacity_without_committing_engine_output() {
    let config = AppConfig::default();
    for failure in 0..5 {
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
                _ => unreachable!(),
            }
        }
        let calls = Cell::new(0);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || STAMP,
            |_| if failure == 4 { None } else { Some(target()) },
            |_, _, _| {
                calls.set(calls.get() + 1);
                false
            }
        ));
        assert_eq!(calls.get(), usize::from(failure == 3));
        assert_eq!(manager.active().unwrap().informative_context(), "teh");
        assert_eq!(manager.active().unwrap().editable_context(), " next");
        assert!(manager
            .active_mut()
            .unwrap()
            .freeze_pending(&config.context)
            .0
            .is_some());
    }
}

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
            |_, _, _| panic!("raced input reached replacement")
        ));
    }
}

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

#[test]
fn input_processor_defers_frozen_completion_until_hook_input_is_drained() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "hello");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit_frozen(&mut pipeline, &mut manager, &config);
    wait_completion(&pipeline);
    let mut processor = crate::background::InputProcessor {
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

fn finish(
    pipeline: &mut CorrectionPipeline,
    manager: &mut SessionManager,
    config: &AppConfig,
    stamp: InputStamp,
    replaced: &Cell<usize>,
    success: bool,
) -> bool {
    pipeline.finish(
        manager,
        &config.context,
        || stamp,
        |_| Some(target()),
        |_, _, _| {
            replaced.set(replaced.get() + 1);
            success
        },
    )
}

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

#[test]
fn security_and_input_changes_during_live_validation_block_replacement() {
    let config = AppConfig::default();
    for change in 0..4 {
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
                    _ => unreachable!(),
                }
                Some(target)
            },
            |_, _, _| panic!("invalid result reached replacement")
        ));
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
    }
}

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

#[test]
fn unchanged_success_commits_only_after_validation() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "hello");
    let mut pipeline = CorrectionPipeline::new().unwrap();
    submit(&mut pipeline, &manager, &config);
    wait_completion(&pipeline);
    assert!(pipeline.finish(
        &mut manager,
        &config.context,
        || STAMP,
        |_| Some(target()),
        |_, _, _| panic!("unchanged text needs no replacement")
    ));
    assert_eq!(manager.active().unwrap().informative_context(), "hello");
    assert_eq!(manager.active().unwrap().editable_context(), "");
}

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
    pipeline.invalidate(&manager, STAMP);
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

#[test]
fn selected_text_completes_without_importing_following_text() {
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
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(manager.active().unwrap().informative_context(), "the");
    assert_eq!(manager.active().unwrap().editable_context(), "");
    assert!(manager
        .active_mut()
        .unwrap()
        .undo_last_correction(&config.context));
}

#[test]
fn unchanged_selection_still_requires_target_confirmation() {
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
    assert_eq!(calls.get(), 1);
    assert_eq!(manager.active().unwrap().editable_context(), "hello later");
    assert_eq!(manager.active().unwrap().informative_context(), "");
}
