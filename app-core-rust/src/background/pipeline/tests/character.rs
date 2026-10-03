//! Configured character boundaries, frozen execution, and completion policy.
use super::*;

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.triggers.word_count_enabled = false;
    config.correction.uncertain_language_policy =
        crate::correction::UncertainLanguagePolicy::CorrectNormally;
    config
}

/// The default period is optional; custom Unicode and multi-key boundaries work.
#[test]
fn configured_boundaries_complete_the_active_segment() {
    for (enabled, correction_enabled, boundary) in [
        (true, true, "."),
        (false, true, "."),
        (true, false, "."),
        (true, true, "?"),
        (true, true, "。"),
        (true, true, "?!"),
    ] {
        let mut config = config();
        assert!(config.triggers.character_trigger_enabled);
        assert_eq!(config.triggers.characters, ["."]);
        config.triggers.character_trigger_enabled = enabled;
        config.correction.enabled = correction_enabled;
        config.triggers.characters = vec![boundary.into()];
        let mut processor = processor(config, CorrectionPipeline::new().unwrap());
        processor
            .session_manager
            .set_informative_context("old. ".into());
        assert!(type_text(&mut processor, "teh word").is_empty());
        let pending = type_text(&mut processor, boundary);
        assert_eq!(pending.len(), usize::from(enabled && correction_enabled));
        let session = processor.session_manager.active().unwrap();
        let original = format!("teh word{boundary}");
        if enabled && correction_enabled {
            let request = &pending[0].request;
            assert_eq!(request.trigger, TriggerKind::Character);
            assert_eq!(request.executable_context, original);
            assert_eq!(request.informative_context, "old. ");
            assert!(session.editable_context().is_empty());
            assert!(session.pending_matches(request.pending_segment_id.unwrap(), &original));
        } else {
            assert_eq!(session.editable_context(), original);
        }
    }
    let mut config = config();
    config.triggers.characters = vec!["?".into(), "!".into()];
    let mut processor = processor(config, CorrectionPipeline::new().unwrap());
    assert!(type_text(&mut processor, "teh.").is_empty());
    assert_eq!(
        type_text(&mut processor, "?")[0].request.executable_context,
        "teh.?"
    );
}

/// Later sentences and unfinished typing stay independent while the engine waits.
#[test]
fn delayed_character_correction_preserves_new_segments_and_shrinks_context() {
    let mut config = config();
    config.triggers.characters = vec!["?".into(), "!".into()];
    config.context.pending_queue_size = 2;
    config.context.informative_context_max_chars = 12;
    config.context.informative_context_min_words = 2;
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let pipeline = CorrectionPipeline::start(move |job| {
        started_tx.send(job.input.clone()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        CorrectionEngines::default().correct_with(job.request.engine, &job.input)
    })
    .unwrap();
    let mut processor = processor(config, pipeline);
    processor
        .session_manager
        .set_informative_context("old words. ".into());
    let first = type_text(&mut processor, "teh word?").remove(0).request;
    let id = first.pending_segment_id.unwrap();
    assert!(dispatch(&mut processor, first));
    let input = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(input.executable_context, "teh word?");
    assert_eq!(input.informative_context, "old words. ");
    let second = type_text(&mut processor, " hello!").remove(0).request;
    let second_id = second.pending_segment_id.unwrap();
    assert_eq!(second.informative_context, "old words. teh word?");
    assert_eq!(second.executable_context, " hello!");
    assert!(dispatch(&mut processor, second));
    assert!(type_text(&mut processor, " 尾 new").is_empty());
    let newer = InputStamp {
        sequence: STAMP.sequence + 20,
        ..STAMP
    };
    for changed in [true, false] {
        release_tx.send(()).unwrap();
        wait_completion(&processor.pipeline);
        let calls = Cell::new(0);
        assert!(finish(
            &mut processor.pipeline,
            &mut processor.session_manager,
            &processor.config,
            newer,
            &calls,
            true
        ));
        assert_eq!(calls.get(), usize::from(changed));
        let session = processor.session_manager.active().unwrap();
        assert_eq!(session.editable_context(), " 尾 new");
        assert!(session.informative_context().chars().count() <= 12);
        assert!(session.informative_context().ends_with(if changed {
            "the word?"
        } else {
            " hello!"
        }));
        assert!(!session.pending_matches(id, "teh word?"));
        if changed {
            assert_eq!(
                started_rx
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .executable_context,
                " hello!"
            );
        }
    }
    let session = processor.session_manager.active().unwrap();
    assert!(!session.pending_matches(second_id, " hello!"));
    assert!(!processor.pipeline.is_correcting());
    assert_eq!(session.informative_context(), "word? hello!");
}

/// High confidence applies, medium follows settings, and low keeps the original.
#[test]
fn character_confidence_policy_commits_checked_text_without_applying_skips() {
    for tier in [
        ConfidenceTier::High,
        ConfidenceTier::Medium,
        ConfidenceTier::Low,
    ] {
        for behavior in [
            ConfidenceBehavior::Silent,
            ConfidenceBehavior::Suggestion,
            ConfidenceBehavior::DoNothing,
        ] {
            let mut config = config();
            config.correction.medium_confidence_behavior = behavior;
            let pipeline = CorrectionPipeline::start(move |job| {
                let mut output = CorrectionOutput::changed("the.".into(), tier, None, 0);
                let disposition = job.input.confidence_behavior.behavior_for(
                    tier,
                    job.input.trigger_type,
                    job.input.suggestion_ui_available,
                );
                if disposition == ConfidenceBehavior::DoNothing {
                    output = CorrectionOutput::unchanged(
                        job.input.executable_context.clone(),
                        tier,
                        NoChangeReason::ConfidenceBelowConfiguredBehavior,
                        0,
                    );
                } else {
                    output.behavior = disposition;
                }
                output
            })
            .unwrap();
            let mut processor = processor(config, pipeline);
            let request = type_text(&mut processor, "teh.").remove(0).request;
            let id = request.pending_segment_id.unwrap();
            assert!(dispatch(&mut processor, request));
            assert!(type_text(&mut processor, " new").is_empty());
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
            let changed = tier == ConfidenceTier::High
                || (tier == ConfidenceTier::Medium && behavior == ConfidenceBehavior::Silent);
            assert_eq!(calls.get(), usize::from(changed));
            let session = processor.session_manager.active().unwrap();
            assert_eq!(
                session.informative_context(),
                if changed { "the." } else { "teh." }
            );
            assert_eq!(session.editable_context(), " new");
            assert_eq!(session.undo_target().is_some(), changed);
            assert!(!session.pending_matches(id, "teh."));
            assert!(processor.pipeline.take_suggestion().is_none());
            assert!(!processor.pipeline.is_correcting());
        }
    }
}

/// Both trigger permissions and hard field-security refusals prevent engine work.
#[test]
fn character_dispatch_respects_rules_and_security_blocks() {
    for secure in [false, true] {
        let pipeline =
            CorrectionPipeline::start(|_| panic!("denied character request reached engine"))
                .unwrap();
        let mut processor = processor(config(), pipeline);
        processor
            .database
            .app_rules()
            .upsert(&crate::storage::AppRule {
                process_name: "notepad.exe".into(),
                window_title_pattern: None,
                list_behavior: "allowlist".into(),
                manual_shortcut_allowed: true,
                word_count_trigger_allowed: true,
                character_trigger_allowed: secure,
                local_engine_allowed: true,
                api_engine_allowed: true,
                safety_mode: "auto".into(),
                prose_context_allowed: false,
            })
            .unwrap();
        let request = type_text(&mut processor, "teh.").remove(0).request;
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
        let session = processor.session_manager.active().unwrap();
        assert_eq!(session.editable_context(), "teh.");
        assert!(session.informative_context().is_empty());
        assert!(!processor.pipeline.is_correcting());
    }
}

/// A frozen result cannot apply to changed text, another control, or unsafe input.
#[test]
fn character_completion_rejects_conflicts_changed_control_security_and_input() {
    for failure in 0..6 {
        let mut processor = processor(config(), CorrectionPipeline::new().unwrap());
        let request = type_text(&mut processor, "teh.").remove(0).request;
        assert!(dispatch(&mut processor, request));
        assert!(type_text(&mut processor, " new").is_empty());
        wait_completion(&processor.pipeline);
        let stamp = Cell::new(STAMP);
        assert!(!processor.pipeline.finish(
            &mut processor.session_manager,
            &processor.config.context,
            || stamp.get(),
            |_| {
                let mut target = target();
                match failure {
                    0 => return None,
                    1 => {
                        target.focused_element_id =
                            Some(FocusedElementId::RuntimeId("other".into()))
                    }
                    2 => target.is_password_or_protected = true,
                    3 => target.process_id += 1,
                    _ => {}
                }
                Some(target)
            },
            |_, _, _| {
                if failure == 5 {
                    stamp.set(InputStamp {
                        sequence: STAMP.sequence + 1,
                        ..STAMP
                    });
                }
                Some(
                    if failure == 4 {
                        "changed. new"
                    } else {
                        "teh. new"
                    }
                    .into(),
                )
            },
            |_, _, _| -> bool { panic!("unsafe character result reached replacement") }
        ));
        let session = processor.session_manager.active().unwrap();
        assert_eq!(session.editable_context(), "teh. new");
        assert!(session.informative_context().is_empty());
        assert!(session.undo_target().is_none());
        assert!(!processor.pipeline.is_correcting());
    }
}
