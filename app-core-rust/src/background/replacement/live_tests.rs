//! Opt-in real-app tests for an explicitly prepared disposable unsent draft.
//! Synthetic session seeding tests the pipeline/native boundary, not physical hooks.
use crate::background::{
    feedback, input_listener, pipeline::CorrectionPipeline, process_group::EngineLease,
    session::SessionManager, target, typing::TypedInput, InputProcessor,
};
use crate::{dictionary::Learner, settings::AppConfig, storage::Database};
use std::time::{Duration, Instant};
use windows::Win32::UI::Accessibility::{IUIAutomationTextPattern, UIA_TextPatternId};

/// Read only the focused field's bounded range; never dump document text.
fn matches_draft(expected: &str) -> bool {
    unsafe {
        let Ok(automation) = target::create_automation() else {
            return false;
        };
        let Ok(editor) = target::resolve_focused_text(&automation) else {
            return false;
        };
        let Ok(pattern) = editor.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        else {
            return false;
        };
        let Ok(field) = pattern
            .RangeFromChild(&editor)
            .or_else(|_| pattern.DocumentRange())
        else {
            return false;
        };
        field.GetText(4097).is_ok_and(|text| text == expected)
    }
}

#[test]
#[ignore = "requires AUTOFIX_EXPECTED_PROCESS and an exact documented synthetic unsent draft; corrects then app-undoes it"]
fn live_composer_roundtrip() {
    let expected = std::env::var("AUTOFIX_EXPECTED_PROCESS").expect("intended app required");
    assert!(matches!(
        expected.as_str(),
        "WhatsApp.Root.exe" | "Telegram.exe"
    ));
    let _lease = EngineLease::acquire().expect("close any competing engine first");
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    let _scope = target::ResolutionScope::begin();
    let target::TargetDetection::Available(target) = target::detect_focused_target() else {
        panic!("focused target unavailable");
    };
    assert!(target.process_name.eq_ignore_ascii_case(&expected));
    assert_eq!(
        target.correction_eligibility(),
        target::CorrectionEligibility::Allowed
    );
    let case = std::env::var("AUTOFIX_LIVE_CASE").unwrap_or_else(|_| "manual".into());
    let (original, corrected, typed, prefix) = match case.as_str() {
        "manual" => ("teh", "the", "teh", ""),
        "word_count" => ("teh word ", "the word ", "teh word ", ""),
        "character" => ("teh.", "the.", "teh.", ""),
        "unicode" => ("é😃 العربية teh", "é😃 العربية the", "é😃 العربية teh", ""),
        "multiline" => ("teh\nword", "the\nword", "teh\nword", ""),
        "surrounding" => (
            "Earlier notes: teh",
            "Earlier notes: the",
            "teh",
            "Earlier notes: ",
        ),
        "newer" => ("teh word next", "the word next", "teh word next", ""),
        "suffix" => ("teh AFTER", "the AFTER", "teh", ""),
        _ => panic!("unknown fixture"),
    };
    assert!(
        matches_draft(original),
        "draft is not the exact synthetic fixture; no mutation attempted"
    );
    let test_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target")
        .join(format!("live-composer-{}", std::process::id()));
    std::fs::create_dir_all(&test_root).unwrap();
    let database = Database::open(&test_root.join("isolated.sqlite")).unwrap();
    let mut config = AppConfig::default();
    config.correction.preferred_language = Some("en".into());
    config.triggers.word_count_enabled = matches!(case.as_str(), "word_count" | "newer");
    config.triggers.word_count = 2;
    config.triggers.character_trigger_enabled = case == "character";
    config.triggers.characters = vec![".".into()];
    config.shortcuts.correct_arbitrary_selection = false;
    config.learning.mode = crate::settings::LearningMode::Off;
    config.feedback.show_correction_applied_notification = false;
    let mut processor = InputProcessor {
        feedback: feedback::Feedback::default(),
        learner: Learner::default(),
        pipeline: CorrectionPipeline::with_database(&database).unwrap(),
        processed_input_sequence: input_listener::current_input_sequence(),
        session_manager: SessionManager::new(config.context.clone()),
        config,
        database,
    };
    // The fixture text was entered through Computer Use. Production hooks still
    // reject injected input; no runtime option can adopt existing field text.
    processor.session_manager.focus(&target);
    processor
        .session_manager
        .set_informative_context(prefix.into());
    let mut pending = Vec::new();
    for ch in typed.chars() {
        processor.track_input(TypedInput::Text(ch.to_string()), &mut pending);
        for work in pending.drain(..) {
            processor.dispatch_trigger(work.request, InputProcessor::input_stamp());
        }
    }
    if !matches!(case.as_str(), "word_count" | "character" | "newer") {
        processor.process_shortcut(1);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while processor.pipeline.is_correcting() {
        processor.pipeline.wait_manual();
        processor.finish_correction();
        assert!(Instant::now() < deadline, "pipeline completion timed out");
    }
    assert!(
        matches_draft(corrected),
        "correction did not produce the exact fixture"
    );
    assert!(
        processor
            .session_manager
            .active()
            .unwrap()
            .undo_target()
            .is_some(),
        "native correction did not commit undo ownership"
    );
    processor.process_shortcut(2);
    assert!(
        matches_draft(original),
        "AutoFix undo did not restore the exact fixture"
    );
    assert!(processor
        .session_manager
        .active()
        .unwrap()
        .undo_target()
        .is_none());
}
