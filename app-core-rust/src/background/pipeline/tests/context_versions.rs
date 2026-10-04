//! Exercise real result validation with a ready completion, without a worker or sleeps.

use super::*;

fn ready_pipeline(manager: &SessionManager, request: CorrectionRequest) -> CorrectionPipeline {
    let active = ActiveRequest {
        id: 1,
        request,
        target: target(),
        editable_snapshot: manager.active().unwrap().editable_context(),
        stamp: STAMP,
        cancelled: Arc::new(AtomicBool::new(false)),
        show_timeout_notice: false,
        trigger_type: TriggerType::ManualShortcut,
        suggestion_ui_available: false,
    };
    let mut output = CorrectionOutput::changed("the".into(), ConfidenceTier::High, None, 0);
    output.behavior = ConfidenceBehavior::Silent;
    CorrectionPipeline {
        database_path: None,
        mailbox: Arc::new((
            Mutex::new(Mailbox {
                completion: Some(Completion { id: 1, output }),
                ..Mailbox::default()
            }),
            Condvar::new(),
        )),
        worker: None,
        active: VecDeque::from([active]),
        next_id: 2,
        timeout_notice: false,
        feedback_event: None,
        suggestion_preview: None,
        preview_cancelled: Arc::new(AtomicBool::new(false)),
    }
}

fn assert_discarded(
    pipeline: &mut CorrectionPipeline,
    manager: &mut SessionManager,
    config: &AppConfig,
) {
    let cancelled = Arc::clone(&pipeline.active.front().unwrap().cancelled);
    assert!(!pipeline.finish(
        manager,
        &config.context,
        || STAMP,
        |_| panic!("stale result reached target validation"),
        |_, _, _| panic!("stale result reached native capture"),
        |_, _, _| -> bool { panic!("stale result reached replacement") },
    ));
    assert!(cancelled.load(Ordering::Acquire));
    assert!(pipeline.active.is_empty());
    assert!(pipeline.mailbox.0.lock().unwrap().completion.is_none());
    assert!(pipeline.take_feedback().is_none());
    assert!(pipeline.take_suggestion().is_none());
    assert!(!manager
        .active_mut()
        .unwrap()
        .undo_last_correction(&config.context));
}

#[test]
fn each_manual_context_version_independently_blocks_a_ready_result() {
    let config = AppConfig::default();
    // Matching snapshot is the positive control for this completion fixture.
    let mut manager = manager(&config, "teh");
    let mut pipeline = ready_pipeline(&manager, request(&manager, &config));
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

    for component in ["context", "executable", "caret_anchor"] {
        let mut manager = super::manager(&config, "teh");
        let mut request = request(&manager, &config);
        match component {
            "context" => request.versions.context += 1,
            "executable" => request.versions.executable += 1,
            "caret_anchor" => request.versions.caret_anchor += 1,
            _ => unreachable!(),
        }
        let mut pipeline = ready_pipeline(&manager, request);
        assert_discarded(&mut pipeline, &mut manager, &config);
        assert_eq!(manager.active().unwrap().editable_context(), "teh");
        assert!(manager.active().unwrap().informative_context().is_empty());
    }
}

#[test]
fn typing_then_backspace_discards_ready_result_despite_identical_text_and_input_stamp() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let mut pipeline = ready_pipeline(&manager, request(&manager, &config));
    let versions = manager.active().unwrap().versions();
    manager.input(TypedInput::Text("!".into()));
    manager.input(TypedInput::Backspace);
    let session = manager.active().unwrap();
    assert_eq!(session.editable_context(), "teh");
    assert_ne!(session.versions().context, versions.context);
    assert_eq!(session.versions().caret_anchor, versions.caret_anchor);
    assert_discarded(&mut pipeline, &mut manager, &config);
    assert_eq!(manager.active().unwrap().editable_context(), "teh");
}

#[test]
fn frozen_result_survives_newer_typing_with_changed_context_version() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    let mut request = request(&manager, &config);
    let id = manager
        .active_mut()
        .unwrap()
        .freeze_pending(&config.context)
        .0
        .unwrap();
    request.pending_segment_id = Some(id);
    request.trigger = TriggerKind::Character;
    let mut pipeline = ready_pipeline(&manager, request);
    pipeline.active.front_mut().unwrap().trigger_type = TriggerType::Character;
    let versions = manager.active().unwrap().versions();
    manager.input(TypedInput::Text(" 新".into()));
    let session = manager.active().unwrap();
    assert_ne!(session.versions().context, versions.context);
    let newer_stamp = InputStamp {
        sequence: STAMP.sequence + 1,
        ..STAMP
    };
    assert!(pipeline.active.front().unwrap().valid(session, newer_stamp));
    let calls = Cell::new(0);
    assert!(finish(
        &mut pipeline,
        &mut manager,
        &config,
        newer_stamp,
        &calls,
        true
    ));
    assert_eq!(calls.get(), 1);
    let session = manager.active().unwrap();
    assert_eq!(session.informative_context(), "the");
    assert_eq!(session.editable_context(), " 新");
    assert!(!session.pending_matches(id, "teh"));
}
