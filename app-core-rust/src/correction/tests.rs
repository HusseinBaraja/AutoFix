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
        suggestion_ui_available: false,
        confidence_behavior: ConfidenceBehaviorSettings {
            high: ConfidenceBehavior::Silent,
            medium: ConfidenceBehavior::Suggestion,
            low: ConfidenceBehavior::DoNothing,
        },
    }
}

#[test]
fn default_confidence_policy_covers_every_trigger_and_ui_capability() {
    let settings = ConfidenceBehaviorSettings::default();
    for trigger in [
        TriggerType::ManualShortcut,
        TriggerType::WordCount,
        TriggerType::Character,
        TriggerType::FinalFixBeforeReanchor,
    ] {
        for ui_available in [false, true] {
            assert_eq!(
                settings.behavior_for(ConfidenceTier::High, trigger, ui_available),
                ConfidenceBehavior::Silent,
            );
            assert_eq!(
                settings.behavior_for(ConfidenceTier::Medium, trigger, ui_available),
                if trigger == TriggerType::ManualShortcut && ui_available {
                    ConfidenceBehavior::Suggestion
                } else {
                    ConfidenceBehavior::DoNothing
                },
            );
            assert_eq!(
                settings.behavior_for(ConfidenceTier::Low, trigger, ui_available),
                ConfidenceBehavior::DoNothing,
            );
        }
    }
}

#[test]
fn explicit_confidence_choices_are_honored_but_low_is_always_blocked() {
    for configured in [
        ConfidenceBehavior::Silent,
        ConfidenceBehavior::Suggestion,
        ConfidenceBehavior::DoNothing,
    ] {
        let settings = ConfidenceBehaviorSettings {
            high: configured,
            medium: configured,
            low: configured,
        };
        for trigger in [TriggerType::ManualShortcut, TriggerType::Character] {
            for ui in [false, true] {
                let expected_high = if configured == ConfidenceBehavior::Suggestion && !ui {
                    ConfidenceBehavior::DoNothing
                } else {
                    configured
                };
                assert_eq!(
                    settings.behavior_for(ConfidenceTier::High, trigger, ui),
                    expected_high
                );
                let expected_medium = if configured == ConfidenceBehavior::Suggestion
                    && (!ui || trigger != TriggerType::ManualShortcut)
                {
                    ConfidenceBehavior::DoNothing
                } else {
                    configured
                };
                assert_eq!(
                    settings.behavior_for(ConfidenceTier::Medium, trigger, ui),
                    expected_medium
                );
                assert_eq!(
                    settings.behavior_for(ConfidenceTier::Low, trigger, ui),
                    ConfidenceBehavior::DoNothing
                );
            }
        }
    }
}

#[test]
fn suppressed_outputs_discard_edit_text_and_details_and_preserve_latency() {
    let mut request = input(CorrectionMode::TyposOnly);
    request.confidence_behavior.low = ConfidenceBehavior::Silent;
    for tier in [ConfidenceTier::Medium, ConfidenceTier::Low] {
        let output = super::confidence::enforce(
            &request,
            CorrectionOutput::changed(
                "the text".into(),
                tier,
                Some(vec![CorrectionChange {
                    start_char: 0,
                    end_char: 3,
                    original_text: "teh".into(),
                    replacement_text: "the".into(),
                    kind: CorrectionChangeKind::Typo,
                    explanation: None,
                }]),
                42,
            ),
        );
        assert_eq!(output.corrected_executable_text, request.executable_context);
        assert!(!output.changes_needed);
        assert_eq!(output.behavior, ConfidenceBehavior::DoNothing);
        assert_eq!(output.changes, Some(Vec::new()));
        assert_eq!(
            output.no_change_reason,
            Some(NoChangeReason::ConfidenceBelowConfiguredBehavior)
        );
        assert_eq!(output.engine_latency_ms, 42);
    }
}

#[test]
fn failures_and_no_change_results_never_authorize_replacement() {
    let request = input(CorrectionMode::TyposOnly);
    for output in [
        CorrectionOutput::timed_out(request.executable_context.clone(), 700),
        CorrectionOutput::failed(
            request.executable_context.clone(),
            EngineFailure {
                kind: EngineFailureKind::Internal,
                message: "failed".into(),
                retryable: false,
            },
            1,
        ),
        CorrectionOutput::unchanged(
            request.executable_context.clone(),
            ConfidenceTier::High,
            NoChangeReason::NoCorrectionNeeded,
            2,
        ),
    ] {
        assert_eq!(super::confidence::enforce(&request, output.clone()), output);
        assert_eq!(output.behavior, ConfidenceBehavior::DoNothing);
    }
}

#[test]
fn omitted_suggestion_capability_and_output_behavior_fail_closed() {
    let mut json = serde_json::to_value(input(CorrectionMode::TyposOnly)).unwrap();
    json.as_object_mut()
        .unwrap()
        .remove("suggestion_ui_available");
    let request: CorrectionInput = serde_json::from_value(json).unwrap();
    assert!(!request.suggestion_ui_available);
    let mut json = serde_json::to_value(CorrectionOutput::changed(
        "the text".into(),
        ConfidenceTier::High,
        None,
        1,
    ))
    .unwrap();
    json.as_object_mut().unwrap().remove("behavior");
    let output: CorrectionOutput = serde_json::from_value(json).unwrap();
    assert_eq!(output.behavior, ConfidenceBehavior::DoNothing);
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
