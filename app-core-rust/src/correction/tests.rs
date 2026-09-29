use super::*;

fn input(mode: CorrectionMode) -> CorrectionInput {
    CorrectionInput {
        informative_context: "Read only context. ".to_owned(),
        executable_context: "teh text".to_owned(),
        mode,
        enabled_grammar_categories: match mode {
            CorrectionMode::TyposOnly => Vec::new(),
            CorrectionMode::TyposPlusGrammar => {
                vec![GrammarCategory::Agreement, GrammarCategory::Spacing]
            }
        },
        language_info: LanguageInfo {
            primary_language: Some("en-US".to_owned()),
            detected_languages: vec!["en-US".to_owned()],
        },
        mixed_language_policy: MixedLanguagePolicy::DominantLanguageOnly,
        uncertain_language_policy: UncertainLanguagePolicy::default(),
        custom_dictionary: vec!["AutoFix".to_owned()],
        protected_terms: vec!["teh-brand".to_owned()],
        trigger_type: TriggerType::ManualShortcut,
        confidence_behavior: ConfidenceBehaviorSettings {
            high: ConfidenceBehavior::Silent,
            medium: ConfidenceBehavior::Suggestion,
            low: ConfidenceBehavior::DoNothing,
        },
    }
}

#[test]
fn unconfigured_engines_accept_every_correction_mode_without_difficulty_routing() {
    let engines = CorrectionEngines::default();

    for kind in [
        EngineKind::LocalMl,
        EngineKind::OpenAiCompatibleApi,
        EngineKind::CustomApi,
    ] {
        for mode in [CorrectionMode::TyposOnly, CorrectionMode::TyposPlusGrammar] {
            let request = input(mode);
            let output = engines.correct_with(kind, &request);
            assert_eq!(engines.engine(kind).kind(), kind);
            assert_eq!(output.corrected_executable_text, request.executable_context);
            assert_eq!(output.no_change_reason, Some(NoChangeReason::EngineError));
            assert!(matches!(
                output.status,
                EngineStatus::Error(EngineFailure {
                    kind: EngineFailureKind::NotImplemented,
                    ..
                })
            ));
        }
    }
}

#[test]
fn local_rule_engine_accepts_every_correction_mode() {
    let engines = CorrectionEngines::default();

    for mode in [CorrectionMode::TyposOnly, CorrectionMode::TyposPlusGrammar] {
        let output = engines.correct_with(EngineKind::LocalRule, &input(mode));
        assert_eq!(output.corrected_executable_text, "the text");
        assert_eq!(output.status, EngineStatus::Completed);
        assert_eq!(output.confidence, ConfidenceTier::High);
    }
}

#[test]
fn engine_kinds_have_explicit_local_or_api_identity() {
    assert_eq!(EngineKind::LocalRule.backend(), EngineBackend::Local);
    assert_eq!(EngineKind::LocalMl.backend(), EngineBackend::Local);
    assert_eq!(
        EngineKind::OpenAiCompatibleApi.backend(),
        EngineBackend::Api
    );
    assert_eq!(EngineKind::CustomApi.backend(), EngineBackend::Api);
}

#[test]
fn engines_advertise_language_capabilities() {
    let engines = CorrectionEngines::default();
    assert!(engines
        .engine(EngineKind::LocalRule)
        .supports_language("en-US"));
    assert!(!engines
        .engine(EngineKind::LocalRule)
        .supports_language("fr-FR"));
    assert!(!engines.engine(EngineKind::LocalMl).supports_language("en"));
    assert!(engines
        .engine(EngineKind::CustomApi)
        .supports_language("fr-FR"));
}

#[test]
fn output_constructors_keep_change_and_status_fields_consistent() {
    let unchanged = CorrectionOutput::unchanged(
        "text".to_owned(),
        ConfidenceTier::High,
        NoChangeReason::NoCorrectionNeeded,
        4,
    );
    assert!(!unchanged.changes_needed);
    assert_eq!(
        unchanged.no_change_reason,
        Some(NoChangeReason::NoCorrectionNeeded)
    );
    assert_eq!(unchanged.status, EngineStatus::Completed);

    let timeout = CorrectionOutput::timed_out("text".to_owned(), 700);
    assert!(!timeout.changes_needed);
    assert_eq!(timeout.confidence, ConfidenceTier::Low);
    assert_eq!(timeout.no_change_reason, Some(NoChangeReason::TimedOut));
    assert_eq!(timeout.status, EngineStatus::TimedOut);
}

#[test]
fn structured_changes_are_scoped_to_executable_character_offsets() {
    let output = CorrectionOutput::changed(
        "the text".to_owned(),
        ConfidenceTier::High,
        Some(vec![CorrectionChange {
            start_char: 0,
            end_char: 3,
            original_text: "teh".to_owned(),
            replacement_text: "the".to_owned(),
            kind: CorrectionChangeKind::Typo,
            explanation: None,
        }]),
        2,
    );

    assert!(output.changes_needed);
    assert!(output.no_change_reason.is_none());
    assert_eq!(output.changes.unwrap()[0].end_char, 3);
}
