//! Word-count admission, continued typing, and conservative completion as one flow.
use super::*;
use crate::background::{security, InputProcessor, PendingTrigger};

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

fn type_text(processor: &mut InputProcessor, text: &str) -> Vec<PendingTrigger> {
    let mut pending = Vec::new();
    for character in text.chars() {
        processor.track_input(TypedInput::Text(character.to_string()), &mut pending);
    }
    pending
}

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

/// Count only completed executable words; settings control freezing and admission.
#[test]
fn default_ten_words_optional_and_configurable() {
    for (enabled, correction_enabled, threshold) in [
        (true, true, 10),
        (false, true, 10),
        (true, false, 10),
        (true, true, 2),
    ] {
        let mut config = AppConfig::default();
        assert_eq!(config.triggers.word_count, 10);
        config.triggers.character_trigger_enabled = false;
        config.triggers.word_count_enabled = enabled;
        config.triggers.word_count = threshold;
        config.correction.enabled = correction_enabled;
        let mut processor = processor(config, CorrectionPipeline::new().unwrap());
        processor
            .session_manager
            .set_informative_context("many old words never count ".into());
        let text = std::iter::repeat_n("word", usize::from(threshold))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(type_text(&mut processor, &text).is_empty());
        let pending = type_text(&mut processor, " ");
        assert_eq!(pending.len(), usize::from(enabled && correction_enabled));
        let session = processor.session_manager.active().unwrap();
        if enabled && correction_enabled {
            let request = &pending[0].request;
            assert_eq!(request.trigger, TriggerKind::WordCount);
            assert_eq!(request.executable_context, format!("{text} "));
            assert_eq!(request.informative_context, "many old words never count ");
            assert!(session.pending_matches(
                request.pending_segment_id.unwrap(),
                &request.executable_context
            ));
            assert!(session.editable_context().is_empty());
        } else {
            assert_eq!(session.editable_context(), format!("{text} "));
        }
    }
}

/// A blocked engine cannot stop typing, and each threshold owns a separate range.
#[test]
fn thresholds_freeze_separate_segments_while_worker_is_running() {
    let mut config = AppConfig::default();
    config.triggers.character_trigger_enabled = false;
    config.triggers.word_count = 2;
    config.context.pending_queue_size = 2;
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let pipeline = CorrectionPipeline::start(move |job| {
        started_tx.send(job.input.clone()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        CorrectionEngines::default().correct_with(job.request.engine, &job.input)
    })
    .unwrap();
    let mut processor = processor(config, pipeline);
    let first = type_text(&mut processor, "teh word ").remove(0).request;
    let first_id = first.pending_segment_id.unwrap();
    assert!(dispatch(&mut processor, first));
    let input = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(input.executable_context, "teh word ");
    assert!(input.informative_context.is_empty());
    // The engine remains held until after both the next threshold and new typing.
    let second = type_text(&mut processor, "teh next ").remove(0).request;
    let second_id = second.pending_segment_id.unwrap();
    assert_ne!(first_id, second_id);
    assert_eq!(second.informative_context, "teh word ");
    assert_eq!(second.executable_context, "teh next ");
    assert!(dispatch(&mut processor, second));
    assert!(type_text(&mut processor, "尾 new").is_empty());
    assert_eq!(
        processor
            .session_manager
            .active()
            .unwrap()
            .editable_context(),
        "尾 new"
    );
    let newer_stamp = InputStamp {
        sequence: STAMP.sequence + 20,
        ..STAMP
    };
    for expected in ["the word ", "the word the next "] {
        release_tx.send(()).unwrap();
        wait_completion(&processor.pipeline);
        assert!(finish(
            &mut processor.pipeline,
            &mut processor.session_manager,
            &processor.config,
            newer_stamp,
            &Cell::new(0),
            true
        ));
        assert_eq!(
            processor
                .session_manager
                .active()
                .unwrap()
                .informative_context(),
            expected
        );
        assert_eq!(
            processor
                .session_manager
                .active()
                .unwrap()
                .editable_context(),
            "尾 new"
        );
        if expected == "the word " {
            let input = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(input.executable_context, "teh next ");
        }
    }
    assert!(!processor.pipeline.is_correcting());
    let session = processor.session_manager.active().unwrap();
    assert!(!session.pending_matches(first_id, "teh word "));
    assert!(!session.pending_matches(second_id, "teh next "));
    assert_eq!(session.undo_target().unwrap().original, "teh next ");
}

/// Policy/security refusal must release pending ownership without running an engine.
#[test]
fn denied_word_count_dispatch_preserves_typed_original() {
    for secure in [false, true] {
        let mut config = AppConfig::default();
        config.triggers.character_trigger_enabled = false;
        config.triggers.word_count = 1;
        let pipeline =
            CorrectionPipeline::start(|_| panic!("denied request reached engine")).unwrap();
        let mut processor = processor(config, pipeline);
        let rule = crate::storage::AppRule {
            process_name: "notepad.exe".into(),
            window_title_pattern: None,
            list_behavior: "allowlist".into(),
            manual_shortcut_allowed: true,
            word_count_trigger_allowed: secure,
            character_trigger_allowed: true,
            local_engine_allowed: true,
            api_engine_allowed: true,
            safety_mode: "auto".into(),
            prose_context_allowed: false,
        };
        processor.database.app_rules().upsert(&rule).unwrap();
        let request = type_text(&mut processor, "teh ").remove(0).request;
        let id = request.pending_segment_id.unwrap();
        assert!(!processor.dispatch_trigger_with(
            request,
            STAMP,
            || STAMP,
            |trigger, config, database| {
                let mut target = target();
                target.is_password_or_protected = secure;
                security::check_detection(
                    trigger,
                    config,
                    &database.app_rules().list().unwrap(),
                    crate::background::target::TargetDetection::Available(target),
                )
            }
        ));
        assert!(!processor.pipeline.is_correcting());
        let session = processor.session_manager.active().unwrap();
        assert_eq!(session.editable_context(), "teh ");
        assert!(!session.pending_matches(id, "teh "));
        assert!(session.informative_context().is_empty());
    }
}

/// Policy-suppressed originals need full live validation, just like silent edits.
#[test]
fn suppressed_word_count_requires_safe_exact_range_and_matching_policy() {
    for failure in 0..9 {
        let mut config = AppConfig::default();
        config.correction.medium_confidence_behavior = if failure == 6 {
            ConfidenceBehavior::Silent
        } else {
            ConfidenceBehavior::DoNothing
        };
        let mut manager = manager(&config, "accomodate ");
        let mut pipeline = CorrectionPipeline::start(|job| {
            CorrectionOutput::unchanged(
                job.input.executable_context.clone(),
                ConfidenceTier::Medium,
                NoChangeReason::ConfidenceBelowConfiguredBehavior,
                0,
            )
        })
        .unwrap();
        let id = submit_frozen(&mut pipeline, &mut manager, &config);
        manager.input(TypedInput::Text("尾".into()));
        wait_completion(&pipeline);
        if failure == 7 || failure == 8 {
            let mut mailbox = pipeline.mailbox.0.lock().unwrap();
            let output = &mut mailbox.completion.as_mut().unwrap().output;
            if failure == 7 {
                output.changes = None;
            } else {
                output.corrected_executable_text = "accommodate ".into();
            }
        }
        let current = Cell::new(STAMP);
        let accepted = pipeline.finish(
            &mut manager,
            &config.context,
            || current.get(),
            |_| {
                let mut target = target();
                match failure {
                    1 => return None,
                    2 => {
                        target.focused_element_id =
                            Some(FocusedElementId::RuntimeId("other".into()))
                    }
                    3 => target.is_password_or_protected = true,
                    _ => {}
                }
                Some(target)
            },
            |_, _, _| {
                if failure == 5 {
                    current.set(InputStamp {
                        sequence: STAMP.sequence + 1,
                        ..STAMP
                    });
                }
                Some(
                    if failure == 4 {
                        "accommodate 尾"
                    } else {
                        "accomodate 尾"
                    }
                    .into(),
                )
            },
            |_, _, _| -> bool { panic!("suppressed result must not mutate") },
        );
        assert_eq!(accepted, failure == 0, "failure {failure}");
        assert!(!pipeline.is_correcting());
        let session = manager.active().unwrap();
        assert!(!session.pending_matches(id, "accomodate "));
        assert!(session.undo_target().is_none());
        assert_eq!(
            session.informative_context(),
            if accepted { "accomodate " } else { "" }
        );
        assert_eq!(
            session.editable_context(),
            if accepted { "尾" } else { "accomodate 尾" }
        );
    }
}
