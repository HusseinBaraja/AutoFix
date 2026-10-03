use super::*;
use crate::background::replacement::{ReplacedRange, ReplacementMethod};

fn selection_request(
    manager: &SessionManager,
    config: &AppConfig,
    arbitrary: bool,
) -> CorrectionRequest {
    let session = manager.active().unwrap();
    let selection = SelectionCapture::Selected {
        text: "teh".into(),
        preceding: if arbitrary { "Foreign é " } else { "old é " }.into(),
        following: " AFTER".into(),
        executable_prefix: (!arbitrary).then(|| "é ".into()),
    };
    let mut request = triggers::manual(
        session.id(),
        session.informative_context(),
        &session.editable_context(),
        session.versions(),
        &selection,
        config,
    )
    .unwrap();
    request.language_info.primary_language = Some("en".into());
    request
}

/// Both routes receive separate contexts; only the executable selection can change.
#[test]
fn selected_manual_flow_commits_context_undo_and_metadata_once() {
    for engine in [
        EngineKind::LocalRule,
        EngineKind::OpenAiCompatibleApi,
        EngineKind::CustomApi,
    ] {
        for arbitrary in [false, true] {
            let path = std::env::temp_dir().join(format!(
                "autofix-manual-{}-{}.sqlite",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let database = crate::storage::Database::open(&path).unwrap();
            let mut config = AppConfig::default();
            config.shortcuts.correct_arbitrary_selection = arbitrary;
            config.logging.full_text_debug_mode_enabled = true;
            let mut manager = manager(&config, "é teh AFTER");
            manager.set_informative_context("old ".into());
            manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
            let mut request = selection_request(&manager, &config, arbitrary);
            request.engine = engine;
            let prefix = request.informative_context.clone();
            let mut pipeline = CorrectionPipeline::start(move |job| {
                assert_eq!(job.request.engine, engine);
                assert_eq!(job.input.executable_context, "teh");
                assert!(job.input.informative_context.starts_with(if arbitrary {
                    "Foreign é "
                } else {
                    "old é "
                }));
                assert!(job.input.informative_context.ends_with(" AFTER"));
                let mut output =
                    CorrectionOutput::changed("the".into(), ConfidenceTier::High, None, 42);
                output.behavior = ConfidenceBehavior::Silent;
                output
            })
            .unwrap();
            pipeline.database_path = Some(path.clone());
            assert!(pipeline.submit(
                request,
                manager.active().unwrap(),
                target(),
                STAMP,
                &config,
                vec![]
            ));
            wait_completion(&pipeline);
            let calls = Cell::new(0);
            assert!(pipeline.finish(
                &mut manager,
                &config.context,
                || STAMP,
                |_| Some(target()),
                |_, request, _| {
                    assert!(request.selected_text);
                    Some(format!("{}teh", request.informative_context))
                },
                |_, request, output| {
                    calls.set(calls.get() + 1);
                    assert_eq!(request.executable_context, "teh");
                    assert!(request.replacement_following_text.is_empty());
                    assert_eq!(output.corrected_executable_text, "the");
                    ReplacementConfirmation {
                        success: true,
                        method: Some(ReplacementMethod::SendInput),
                        range: Some(ReplacedRange {
                            start_back: 3,
                            end_back: 0,
                        }),
                    }
                }
            ));
            assert_eq!(calls.get(), 1);
            let session = manager.active_mut().unwrap();
            assert_eq!(session.informative_context(), format!("{prefix}the"));
            assert!(session.editable_context().is_empty());
            assert!(!session.position_uncertain());
            let undo = session.undo_target().unwrap();
            assert_eq!(undo.original, "teh");
            assert_eq!(undo.corrected, "the");
            assert!(undo.following.is_empty());
            assert!(session.undo_last_correction(&config.context));
            assert_eq!(session.informative_context(), format!("{prefix}teh"));
            assert!(!session.undo_last_correction(&config.context));
            assert!(!pipeline.finish(
                &mut manager,
                &config.context,
                || STAMP,
                |_| panic!("duplicate target check"),
                |_, _, _| panic!("duplicate capture"),
                |_, _, _| -> bool { panic!("duplicate replacement") }
            ));
            let connection = rusqlite::Connection::open(&path).unwrap();
            let rows: Vec<(String, String, String, u64)> = connection.prepare(
                "select trigger_type, replacement_method, result_reason, latency_ms from correction_metadata"
            ).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
                .unwrap().map(Result::unwrap).collect();
            assert_eq!(
                rows,
                vec![(
                    "manual_shortcut".into(),
                    "send_input".into(),
                    "correction_applied".into(),
                    42
                )]
            );
            let debug_count: u64 = connection
                .query_row("select count(*) from debug_events", [], |row| row.get(0))
                .unwrap();
            assert_eq!(debug_count, 0);
            drop(pipeline);
            drop(connection);
            drop(database);
            std::fs::remove_file(path).unwrap();
        }
    }
}

/// Stale, protected, unproved and failed results cannot commit selection ownership.
#[test]
fn selection_failures_preserve_executable_context_and_have_no_undo() {
    for failure in [
        "capture", "mismatch", "secure", "title", "input", "replace", "low", "medium",
    ] {
        let config = AppConfig::default();
        let mut manager = manager(&config, "é teh AFTER");
        manager.set_informative_context("old ".into());
        let request = selection_request(&manager, &config, false);
        let mut pipeline = CorrectionPipeline::start(move |_| {
            let confidence = match failure {
                "low" => ConfidenceTier::Low,
                "medium" => ConfidenceTier::Medium,
                _ => ConfidenceTier::High,
            };
            let mut output = CorrectionOutput::changed("the".into(), confidence, None, 0);
            output.behavior = ConfidenceBehavior::Silent;
            output
        })
        .unwrap();
        assert!(pipeline.submit(
            request,
            manager.active().unwrap(),
            target(),
            STAMP,
            &config,
            vec![]
        ));
        wait_completion(&pipeline);
        let calls = Cell::new(0);
        assert!(!pipeline.finish(
            &mut manager,
            &config.context,
            || InputStamp {
                sequence: STAMP.sequence + u64::from(failure == "input"),
                ..STAMP
            },
            |_| {
                let mut target = target();
                target.is_password_or_protected = failure == "secure";
                if failure == "title" {
                    target.window_title = "Private".into();
                }
                Some(target)
            },
            |_, _, _| match failure {
                "capture" => None,
                "mismatch" => Some("old é teh AFTER".into()),
                _ => Some("old é teh".into()),
            },
            |_, _, _| {
                calls.set(calls.get() + 1);
                false
            }
        ));
        assert_eq!(calls.get(), usize::from(failure == "replace"));
        assert_eq!(manager.active().unwrap().editable_context(), "é teh AFTER");
        assert_eq!(manager.active().unwrap().informative_context(), "old ");
        assert!(manager.active().unwrap().undo_target().is_none());
    }
}

/// A forged temporary or foreign selection cannot bypass the default ownership gate.
#[test]
fn selected_submission_requires_current_typed_scope_or_arbitrary_opt_in() {
    let mut config = AppConfig::default();
    config.shortcuts.correct_arbitrary_selection = true;
    let manager = manager(&config, "é teh AFTER");
    let mut request = selection_request(&manager, &config, true);
    config.shortcuts.correct_arbitrary_selection = false;
    let mut pipeline =
        CorrectionPipeline::start(|_| panic!("unauthorized selection reached engine")).unwrap();
    assert!(!pipeline.submit(
        request.clone(),
        manager.active().unwrap(),
        target(),
        STAMP,
        &config,
        vec![]
    ));
    request.temporary_selection = false;
    assert!(!pipeline.submit(
        request,
        manager.active().unwrap(),
        target(),
        STAMP,
        &config,
        vec![]
    ));
    assert!(pipeline.active.is_empty());
}

/// Selected medium results preview by policy and apply only with explicit silent behavior.
#[test]
fn selected_medium_confidence_obeys_preview_and_silent_preferences() {
    for silent in [false, true] {
        let mut config = AppConfig::default();
        config.feedback.show_medium_confidence_suggestions = true;
        if silent {
            config.correction.medium_confidence_behavior = ConfidenceBehavior::Silent;
        }
        let mut manager = manager(&config, "é teh AFTER");
        manager.set_informative_context("old ".into());
        let request = selection_request(&manager, &config, false);
        let mut pipeline = CorrectionPipeline::start(|job| {
            let mut output =
                CorrectionOutput::changed("the".into(), ConfidenceTier::Medium, None, 0);
            output.behavior = job.input.confidence_behavior.behavior_for(
                ConfidenceTier::Medium,
                job.input.trigger_type,
                job.input.suggestion_ui_available,
            );
            output
        })
        .unwrap();
        assert!(pipeline.submit(
            request,
            manager.active().unwrap(),
            target(),
            STAMP,
            &config,
            vec![]
        ));
        wait_completion(&pipeline);
        let calls = Cell::new(0);
        let applied = pipeline.finish(
            &mut manager,
            &config.context,
            || STAMP,
            |_| Some(target()),
            |_, _, _| Some("old é teh".into()),
            |_, _, _| {
                calls.set(calls.get() + 1);
                true
            },
        );
        assert_eq!(applied, silent);
        assert_eq!(calls.get(), usize::from(silent));
        let preview = pipeline.take_suggestion();
        let ui_available = PreviewSuggestionUi::new(&config.feedback).is_available();
        assert_eq!(preview.is_some(), !silent && ui_available);
        if let Some(preview) = preview {
            assert_eq!(preview.text, "AutoFix suggestion: the");
        }
        assert_eq!(
            manager.active().unwrap().editable_context(),
            if silent { "" } else { "é teh AFTER" }
        );
        assert_eq!(manager.active().unwrap().undo_target().is_some(), silent);
    }
}

/// Successful unselected manual correction uses the same receipt and clears executable typing.
#[test]
fn collapsed_manual_flow_preserves_informative_context_and_records_undo() {
    let config = AppConfig::default();
    let mut manager = manager(&config, "teh");
    manager.set_informative_context("read only ".into());
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
    assert_eq!(
        manager.active().unwrap().informative_context(),
        "read only the"
    );
    assert!(manager.active().unwrap().editable_context().is_empty());
    assert_eq!(
        manager.active().unwrap().undo_target().unwrap().original,
        "teh"
    );
}
