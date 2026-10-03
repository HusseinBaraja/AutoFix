//! Movement through the runtime owner, worker, live range gate and session commit.
use super::*;

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.triggers.word_count_enabled = false;
    config.context.pending_queue_size = 2;
    config.correction.preferred_language = Some("en".into());
    config.correction.uncertain_language_policy =
        crate::correction::UncertainLanguagePolicy::CorrectNormally;
    config
}

fn move_and_type(
    processor: &mut InputProcessor,
    preceding: &str,
    inserted: &str,
) -> Vec<PendingTrigger> {
    let mut pending = Vec::new();
    processor.track_input(
        TypedInput::Uncertain(MovementSignal::MouseClick),
        &mut pending,
    );
    assert!(processor
        .session_manager
        .active()
        .unwrap()
        .position_uncertain());
    processor.track_input(TypedInput::Text(inserted.into()), &mut pending);
    processor.resolve_caret_movement(Some(preceding), &mut pending);
    pending
}

#[test]
fn short_hop_keeps_typing_executable_and_corrects_only_owned_spans() {
    let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
    assert!(type_text(&mut processor, "teh").is_empty());
    assert!(move_and_type(&mut processor, "teh teh X", "X").is_empty());
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .trigger_context(),
        "teh X"
    );
    let requests = type_text(&mut processor, ".");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].request.executable_context, "teh");
    assert_eq!(requests[1].request.executable_context, "X.");
    assert_eq!(requests[1].request.informative_context, "teh teh ");
    for pending in requests {
        assert!(dispatch(&mut processor, pending.request));
    }
    let document = std::cell::RefCell::new("teh teh X. AFTER".to_owned());
    let calls = Cell::new(0);
    for _ in 0..2 {
        wait_completion(&processor.pipeline);
        let live = document.borrow().strip_suffix(" AFTER").unwrap().to_owned();
        assert!(processor.pipeline.finish(
            &mut processor.session_manager,
            &processor.config.context,
            || STAMP,
            |_| Some(target()),
            |_, _, _| Some(live.clone()),
            |_, request, output| {
                assert_eq!(request.executable_context, "teh");
                assert_eq!(request.replacement_following_text, " teh X.");
                document
                    .borrow_mut()
                    .replace_range(0..3, &output.corrected_executable_text);
                calls.set(calls.get() + 1);
                true
            }
        ));
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(*document.borrow(), "the teh X. AFTER");
    let session = processor.session_manager.active().unwrap();
    assert_eq!(session.informative_context(), "the teh X.");
    assert!(session.executable_context().is_empty());
    assert_eq!(session.undo_target().unwrap().following, " teh X.");
}

#[test]
fn long_hop_runs_final_fix_or_no_change_and_keeps_new_typing() {
    for original in ["teh", "hello"] {
        let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
        type_text(&mut processor, original);
        let requests = move_and_type(
            &mut processor,
            &format!("{original} one two three four five six X"),
            "X",
        );
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].request.trigger,
            TriggerKind::FinalFixBeforeReanchor
        );
        assert_eq!(requests[0].request.executable_context, original);
        assert!(dispatch(
            &mut processor,
            requests.into_iter().next().unwrap().request
        ));
        assert!(type_text(&mut processor, " 尾").is_empty());
        wait_completion(&processor.pipeline);
        let calls = Cell::new(0);
        assert!(finish(
            &mut processor.pipeline,
            &mut processor.session_manager,
            &processor.config,
            InputStamp {
                sequence: STAMP.sequence + 3,
                ..STAMP
            },
            &calls,
            true
        ));
        assert_eq!(calls.get(), usize::from(original == "teh"));
        let session = processor.session_manager.active().unwrap();
        let corrected = if original == "teh" { "the" } else { original };
        assert_eq!(
            session.informative_context(),
            format!("{corrected} one two three four five six ")
        );
        assert_eq!(session.editable_context(), "X 尾");
        assert_eq!(session.undo_target().is_some(), original == "teh");
        if original == "teh" {
            assert_eq!(
                session.undo_target().unwrap().following,
                " one two three four five six X 尾"
            );
        }
    }
}

#[test]
fn movement_before_pending_result_cancels_it_and_submits_one_final_fix() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let pipeline = CorrectionPipeline::start(move |job| {
        started_tx.send(job.input.trigger_type).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        CorrectionEngines::default().correct_with(job.request.engine, &job.input)
    })
    .unwrap();
    let mut processor = processor(config(), pipeline);
    let original = type_text(&mut processor, "teh.").remove(0).request;
    assert!(dispatch(&mut processor, original));
    assert_eq!(
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        TriggerType::Character
    );
    let mut pending = Vec::new();
    processor.track_input(
        TypedInput::Uncertain(MovementSignal::MouseClick),
        &mut pending,
    );
    processor.pipeline.invalidate(
        &mut processor.session_manager,
        InputStamp {
            position: 8,
            ..STAMP
        },
    );
    processor.track_input(TypedInput::Text("X".into()), &mut pending);
    processor.resolve_caret_movement(Some("teh. gap X"), &mut pending);
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].request.trigger,
        TriggerKind::FinalFixBeforeReanchor
    );
    assert!(dispatch(&mut processor, pending.remove(0).request));
    release_tx.send(()).unwrap();
    assert_eq!(
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        TriggerType::FinalFixBeforeReanchor
    );
    release_tx.send(()).unwrap();
    wait_completion(&processor.pipeline);
    let calls = Cell::new(0);
    assert!(finish(
        &mut processor.pipeline,
        &mut processor.session_manager,
        &processor.config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(calls.get(), 1);
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .informative_context(),
        "the. gap "
    );
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .editable_context(),
        "X"
    );
    assert!(!processor.pipeline.is_correcting());
}

#[test]
fn final_fix_rechecks_live_range_and_does_not_retry_a_failed_edit() {
    for changed_range in [false, true] {
        let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
        type_text(&mut processor, "teh");
        let pending = move_and_type(&mut processor, "teh one two three four five six X", "X");
        assert!(dispatch(
            &mut processor,
            pending.into_iter().next().unwrap().request
        ));
        wait_completion(&processor.pipeline);
        let calls = Cell::new(0);
        let live = if changed_range {
            "changed one two three four five six X".into()
        } else {
            processor
                .session_manager
                .active()
                .unwrap()
                .known_before_caret()
        };
        assert!(!processor.pipeline.finish(
            &mut processor.session_manager,
            &processor.config.context,
            || STAMP,
            |_| Some(target()),
            |_, _, _| Some(live),
            |_, _, _| {
                calls.set(calls.get() + 1);
                false
            }
        ));
        assert_eq!(calls.get(), usize::from(!changed_range));
        let session = processor.session_manager.active_mut().unwrap();
        assert!(session
            .movement_requests(None, &processor.config)
            .is_empty());
        assert_eq!(session.editable_context(), "X");
        assert!(session.undo_target().is_none());
    }
}

#[test]
fn policy_denial_and_new_caret_never_apply_the_old_result() {
    for denied in [false, true] {
        let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
        type_text(&mut processor, "teh");
        let pending = move_and_type(&mut processor, "teh one two three four five six X", "X");
        let request = pending.into_iter().next().unwrap().request;
        if denied {
            assert!(!processor.dispatch_trigger_with(
                request,
                STAMP,
                || STAMP,
                |_, _, _| {
                    let mut protected = target();
                    protected.is_password_or_protected = true;
                    security::check_detection(
                        TriggerKind::FinalFixBeforeReanchor,
                        &config(),
                        &[],
                        crate::background::target::TargetDetection::Available(protected),
                    )
                }
            ));
            assert_eq!(
                processor
                    .session_manager
                    .active()
                    .unwrap()
                    .editable_context(),
                "X"
            );
        } else {
            assert!(dispatch(&mut processor, request));
            wait_completion(&processor.pipeline);
            processor
                .session_manager
                .input(TypedInput::Uncertain(MovementSignal::HomeEnd));
            let calls = Cell::new(0);
            assert!(!finish(
                &mut processor.pipeline,
                &mut processor.session_manager,
                &processor.config,
                InputStamp {
                    position: STAMP.position + 1,
                    ..STAMP
                },
                &calls,
                true
            ));
            assert_eq!(calls.get(), 0);
            processor
                .session_manager
                .input(TypedInput::Text("Y".into()));
            processor.resolve_caret_movement(Some("other Y"), &mut Vec::new());
            assert_eq!(
                processor
                    .session_manager
                    .active()
                    .unwrap()
                    .executable_context(),
                "Y"
            );
        }
        assert!(!processor.pipeline.is_correcting());
    }
}

#[test]
fn manual_correction_checks_retained_typing_without_editing_gap() {
    let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
    type_text(&mut processor, "teh");
    move_and_type(&mut processor, "teh gap teh", "teh");
    let active = request(&processor.session_manager, &processor.config);
    let requests = processor.manual_requests(active);
    assert_eq!(requests.len(), 2);
    for mut request in requests {
        request.language_info = crate::correction::LanguageInfo {
            primary_language: Some("en".into()),
            detected_languages: vec!["en".into()],
        };
        assert!(processor.pipeline.submit(
            request,
            processor.session_manager.active().unwrap(),
            target(),
            STAMP,
            &processor.config,
            Vec::new()
        ));
    }
    let calls = Cell::new(0);
    for _ in 0..2 {
        wait_completion(&processor.pipeline);
        assert!(finish(
            &mut processor.pipeline,
            &mut processor.session_manager,
            &processor.config,
            STAMP,
            &calls,
            true
        ));
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .informative_context(),
        "the gap the"
    );
}

#[test]
fn word_threshold_counts_retained_typing_and_excludes_skipped_words() {
    let mut settings = config();
    settings.triggers.character_trigger_enabled = false;
    settings.triggers.word_count_enabled = true;
    settings.triggers.word_count = 2;
    let mut processor = processor(settings, CorrectionPipeline::new().unwrap());
    type_text(&mut processor, "teh ");
    assert!(move_and_type(&mut processor, "teh  one two three four five next", "next").is_empty());
    let pending = type_text(&mut processor, " ");
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].request.executable_context, "teh ");
    assert_eq!(pending[1].request.executable_context, "next ");
}

#[test]
fn default_single_slot_drains_retained_spans_in_order() {
    let mut settings = config();
    settings.context.pending_queue_size = 1;
    let mut processor = processor(settings, CorrectionPipeline::new().unwrap());
    type_text(&mut processor, "teh");
    move_and_type(&mut processor, "teh gap X", "X");
    let mut pending = type_text(&mut processor, ".");
    assert_eq!(pending.len(), 1);
    assert!(dispatch(&mut processor, pending.remove(0).request));
    wait_completion(&processor.pipeline);
    let calls = Cell::new(0);
    assert!(finish(
        &mut processor.pipeline,
        &mut processor.session_manager,
        &processor.config,
        STAMP,
        &calls,
        true
    ));
    let requests = processor
        .session_manager
        .active_mut()
        .unwrap()
        .movement_requests(None, &processor.config);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].executable_context, "X.");
    assert!(dispatch(
        &mut processor,
        requests.into_iter().next().unwrap()
    ));
    wait_completion(&processor.pipeline);
    assert!(finish(
        &mut processor.pipeline,
        &mut processor.session_manager,
        &processor.config,
        STAMP,
        &calls,
        true
    ));
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .informative_context(),
        "the gap X."
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn failed_retained_work_cancels_dependents_and_waits_for_another_trigger() {
    let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
    type_text(&mut processor, "teh");
    move_and_type(&mut processor, "teh gap X", "X");
    for pending in type_text(&mut processor, ".") {
        assert!(dispatch(&mut processor, pending.request));
    }
    wait_completion(&processor.pipeline);
    let calls = Cell::new(0);
    assert!(!finish(
        &mut processor.pipeline,
        &mut processor.session_manager,
        &processor.config,
        STAMP,
        &calls,
        false
    ));
    assert!(!processor.pipeline.is_correcting());
    let session = processor.session_manager.active_mut().unwrap();
    assert!(session
        .movement_requests(None, &processor.config)
        .is_empty());
    assert_eq!(session.executable_context(), "tehX.");
    assert_eq!(session.known_before_caret(), "teh gap X.");
}
