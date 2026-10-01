use std::{fs, time::SystemTime};

use crate::correction::{ConfidenceBehavior, CorrectionMode, GrammarCategory};

use super::{
    load_config,
    model::{CorrectionEngine, RunMode},
    save_config,
    toml_io::config_to_toml,
    AppConfig, ValidateConfig,
};

#[test]
fn learning_is_opt_in_and_round_trips_with_legacy_defaults() {
    use super::{LearningMode, LearningRule};
    let mut config = AppConfig::default();
    assert_eq!(config.learning.mode, LearningMode::Off);
    for mode in [
        LearningMode::Off,
        LearningMode::Ask,
        LearningMode::Automatic,
    ] {
        for rule in [LearningRule::Dictionary, LearningRule::Pair] {
            config.learning.mode = mode;
            config.learning.rule = rule;
            config.learning.per_app = true;
            let encoded = config_to_toml(&config).unwrap();
            assert_eq!(
                super::toml_io::parse_config(&encoded).unwrap().learning,
                config.learning
            );
        }
    }
    let mut document = toml::Value::try_from(&config).unwrap();
    document.as_table_mut().unwrap().remove("learning");
    let legacy = toml::to_string(&document).unwrap();
    assert_eq!(
        super::toml_io::parse_config(&legacy).unwrap().learning.mode,
        LearningMode::Off
    );
    assert!(super::toml_io::parse_config(
        &legacy.replace("[general]", "[learning]\nmode = 'invalid'\n[general]")
    )
    .is_err());
}

#[test]
fn clipboard_preference_round_trips_and_legacy_configs_keep_default() {
    let mut config = AppConfig::default();
    assert!(config.replacement.clipboard_enabled);
    config.replacement.clipboard_enabled = false;
    let encoded = config_to_toml(&config).unwrap();
    assert!(
        !super::toml_io::parse_config(&encoded)
            .unwrap()
            .replacement
            .clipboard_enabled
    );
    let legacy = encoded
        .lines()
        .filter(|line| !line.starts_with("clipboard_enabled"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        super::toml_io::parse_config(&legacy)
            .unwrap()
            .replacement
            .clipboard_enabled
    );
    let legacy = legacy.replace("[replacement]\n", "");
    assert!(
        super::toml_io::parse_config(&legacy)
            .unwrap()
            .replacement
            .clipboard_enabled
    );
}

/// Product defaults preserve conservative context, language, grammar, and confidence settings.
#[test]
fn default_config_has_requested_values() {
    let config = AppConfig::default();

    assert_eq!(config.general.run_mode, RunMode::Blocklist);
    assert_eq!(config.shortcuts.correct, "Ctrl+Alt+Space");
    assert_eq!(config.shortcuts.undo, "Ctrl+Alt+Z");
    assert!(!config.shortcuts.correct_arbitrary_selection);
    assert_eq!(config.triggers.word_count, 10);
    assert_eq!(config.triggers.characters, vec!["."]);
    assert_eq!(config.context.initial_context_words, 25);
    assert_eq!(config.context.initial_context_boundary_chars, vec!["."]);
    assert_eq!(config.context.forward_movement_word_limit, 5);
    assert_eq!(config.context.informative_context_min_words, 25);
    assert_eq!(config.context.pending_queue_size, 1);
    assert_eq!(
        config.context.pending_queue_full_behavior,
        super::PendingQueueFullBehavior::SkipNew
    );
    assert!(!config.onboarding.completed);
    assert!(config.correction.enabled);
    assert_eq!(config.correction.mode, CorrectionMode::TyposOnly);
    assert_eq!(config.correction.engine, CorrectionEngine::Local);
    assert_eq!(config.correction.preferred_language, None);
    assert!(config.correction.app_language_overrides.is_empty());
    assert_eq!(
        config.correction.uncertain_language_policy,
        crate::correction::UncertainLanguagePolicy::HighConfidenceTyposOnly
    );
    assert_eq!(
        config.correction.mixed_language_policy,
        crate::correction::MixedLanguagePolicy::DominantLanguageOnly
    );
    assert_eq!(
        config.correction.high_confidence_behavior,
        ConfidenceBehavior::Silent
    );
    assert_eq!(
        config.correction.medium_confidence_behavior,
        ConfidenceBehavior::Suggestion
    );
    assert_eq!(
        config.correction.low_confidence_behavior,
        ConfidenceBehavior::DoNothing
    );
    assert_eq!(config.api.timeout_manual_ms, 3_000);
    assert_eq!(config.api.timeout_auto_ms, 700);
    assert_eq!(config.api.retry_count, 1);
    assert_eq!(config.api.temperature, 0.0);
    assert!(!config.api.streaming);
    assert!(!config.api.fallback_to_local);
    assert!(!config.logging.debug_mode_enabled);
    assert!(!config.logging.redacted_debug_mode_enabled);
    assert!(!config.logging.full_text_debug_mode_enabled);
    assert_eq!(config.logging.log_retention_days, None);
}

/// Save validation remains strict after legacy retry counts are migrated on read.
#[test]
fn api_retry_count_accepts_only_zero_or_one() {
    let mut config = AppConfig::default();
    for retries in [0, 1] {
        config.api.retry_count = retries;
        let encoded = config_to_toml(&config).unwrap();
        assert_eq!(
            super::toml_io::parse_config(&encoded)
                .unwrap()
                .api
                .retry_count,
            retries
        );
    }
    for retries in [2, 255] {
        config.api.retry_count = retries;
        assert_eq!(config.validate().unwrap_err().field(), "api.retry_count");
        assert!(config_to_toml(&config).is_err());
        let legacy = toml::to_string(&config).unwrap();
        let loaded = super::toml_io::parse_config(&legacy).unwrap();
        assert_eq!(loaded.api.retry_count, 1);
        let saved = config_to_toml(&loaded).unwrap();
        assert_eq!(super::toml_io::parse_config(&saved).unwrap(), loaded);
    }
    for manual in [false, true] {
        let mut config = AppConfig::default();
        let field = if manual {
            config.api.timeout_manual_ms = 0;
            "api.timeout_manual_ms"
        } else {
            config.api.timeout_auto_ms = 0;
            "api.timeout_auto_ms"
        };
        assert_eq!(config.validate().unwrap_err().field(), field);
    }
}

#[test]
fn pending_queue_settings_round_trip_and_legacy_configs_get_safe_defaults() {
    for behavior in [
        super::PendingQueueFullBehavior::SkipNew,
        super::PendingQueueFullBehavior::CancelOldest,
        super::PendingQueueFullBehavior::MergeNewest,
    ] {
        let mut config = AppConfig::default();
        config.context.pending_queue_size = 4;
        config.context.pending_queue_full_behavior = behavior;
        let toml = config_to_toml(&config).unwrap();
        let restored = super::toml_io::parse_config(&toml).unwrap();
        assert_eq!(restored.context, config.context);
        let legacy = toml
            .lines()
            .filter(|line| !line.starts_with("pending_queue_"))
            .collect::<Vec<_>>()
            .join("\n");
        let restored = super::toml_io::parse_config(&legacy).unwrap();
        assert_eq!(restored.context.pending_queue_size, 1);
        assert_eq!(
            restored.context.pending_queue_full_behavior,
            super::PendingQueueFullBehavior::SkipNew
        );
    }
    for size in [0, 17, u16::MAX] {
        let mut config = AppConfig::default();
        config.context.pending_queue_size = size;
        assert_eq!(
            config.validate().unwrap_err().field(),
            "context.pending_queue_size"
        );
    }
}

/// Confidence preferences survive TOML while feedback can suppress, never silently apply, suggestions.
#[test]
fn confidence_preferences_round_trip_and_feedback_only_disables_suggestions() {
    let mut config = AppConfig::default();
    let defaults = crate::correction::ConfidenceBehaviorSettings::default();
    assert_eq!(config.confidence_behavior(), defaults);
    config.feedback.show_medium_confidence_suggestions = false;
    assert_eq!(
        config.confidence_behavior().medium,
        ConfidenceBehavior::DoNothing
    );
    config.correction.high_confidence_behavior = ConfidenceBehavior::DoNothing;
    config.correction.medium_confidence_behavior = ConfidenceBehavior::Silent;
    let encoded = config_to_toml(&config).unwrap();
    let loaded = super::toml_io::parse_config(&encoded).unwrap();
    assert_eq!(
        loaded.correction.high_confidence_behavior,
        ConfidenceBehavior::DoNothing
    );
    assert_eq!(
        loaded.confidence_behavior().medium,
        ConfidenceBehavior::Silent
    );
    assert_eq!(
        loaded.confidence_behavior().low,
        ConfidenceBehavior::DoNothing
    );
}

#[test]
fn generated_toml_has_comments_and_no_api_key_field() {
    let output = config_to_toml(&AppConfig::default()).unwrap();

    assert!(output.contains("# AutoFix user configuration."));
    assert!(output.contains("[general]"));
    assert!(output.contains("correct_arbitrary_selection = false"));
    assert!(output.contains("[api]"));
    assert!(!output.to_lowercase().contains("api_key"));
}

#[test]
fn arbitrary_selection_setting_round_trips_and_legacy_config_stays_strict() {
    let mut config = AppConfig::default();
    config.shortcuts.correct_arbitrary_selection = true;
    let encoded = config_to_toml(&config).unwrap();
    assert!(
        super::toml_io::parse_config(&encoded)
            .unwrap()
            .shortcuts
            .correct_arbitrary_selection
    );
    let legacy = encoded.replace("correct_arbitrary_selection = true\n", "");
    assert!(
        !super::toml_io::parse_config(&legacy)
            .unwrap()
            .shortcuts
            .correct_arbitrary_selection
    );
}

/// A full TOML config loads typed correction categories and the remaining user settings.
#[test]
fn parses_full_user_config() {
    let config = super::toml_io::parse_config(
        r#"
[general]
start_with_windows = true
run_mode = "allowlist"

[shortcuts]
correct = "Ctrl+Alt+Space"
undo = "Ctrl+Alt+Z"

[triggers]
word_count_enabled = true
word_count = 12
character_trigger_enabled = true
characters = [".", "?", "!"]

[context]
initial_context_words = 25
initial_context_boundary_chars = [".", "?", "!"]
forward_movement_word_limit = 5
informative_context_max_chars = 2000
informative_context_min_words = 25
executable_context_max_words = 80

[correction]
enabled = true
mode = "typos_plus_grammar"
engine = "api"
high_confidence_behavior = "silent"
medium_confidence_behavior = "suggestion"
low_confidence_behavior = "do_nothing"
enabled_grammar_categories = ["agreement", "punctuation"]

[api]
provider_preset = "custom"
base_url = "https://example.test/v1"
model = "typo-model"
timeout_manual_ms = 3000
timeout_auto_ms = 700
retry_count = 1
fallback_to_local = true
temperature = 0.0
streaming = false

[feedback]
tray_state_enabled = true
show_correction_applied_notification = true
show_skipped_reason = true
show_medium_confidence_suggestions = true
show_blocked_app_notice = true
show_timeout_notice = true

[logging]
metadata_only_logs_enabled = true
debug_mode_enabled = false
redacted_debug_mode_enabled = false
full_text_debug_mode_enabled = false
log_retention_days = 30
"#,
    )
    .unwrap();

    assert_eq!(config.general.run_mode, RunMode::Allowlist);
    assert_eq!(config.triggers.word_count, 12);
    assert!(config.correction.enabled);
    assert_eq!(config.correction.mode, CorrectionMode::TyposPlusGrammar);
    assert_eq!(
        config.correction.enabled_grammar_categories,
        vec![GrammarCategory::Agreement, GrammarCategory::Spacing]
    );
    assert_eq!(config.logging.log_retention_days, Some(30));
}

#[test]
fn rejects_invalid_confidence_behavior() {
    let mut config = AppConfig::default();
    config.correction.low_confidence_behavior = ConfidenceBehavior::Silent;

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "correction.low_confidence_behavior");
}

/// Language settings round-trip and reject malformed tags and duplicate app overrides.
#[test]
fn language_settings_round_trip_and_validate() {
    let mut config = AppConfig::default();
    config.correction.preferred_language = Some("en-US".into());
    config.correction.app_language_overrides = vec!["notepad.exe=fr-FR".into()];
    config.correction.uncertain_language_policy =
        crate::correction::UncertainLanguagePolicy::DoNothing;
    config.correction.mixed_language_policy =
        crate::correction::MixedLanguagePolicy::DisableCorrection;
    let encoded = config_to_toml(&config).unwrap();
    assert_eq!(
        super::toml_io::parse_config(&encoded).unwrap().correction,
        config.correction
    );
    config
        .correction
        .app_language_overrides
        .push("NOTEPAD.EXE=de".into());
    assert_eq!(
        config.validate().unwrap_err().field(),
        "correction.app_language_overrides"
    );
    config.correction.app_language_overrides.clear();
    config.correction.preferred_language = Some("invalid tag".into());
    assert_eq!(
        config.validate().unwrap_err().field(),
        "correction.preferred_language"
    );
}

/// Per-token mixed-language correction requires explicit API engine selection.
#[test]
fn per_token_policy_requires_api_engine() {
    let mut config = AppConfig::default();
    config.correction.mixed_language_policy = crate::correction::MixedLanguagePolicy::PerToken;
    assert_eq!(
        config.validate().unwrap_err().field(),
        "correction.mixed_language_policy"
    );
    config.correction.engine = CorrectionEngine::Api;
    assert!(config.validate().is_ok());
}

/// Persisted settings reject malformed tags in both preferences and app overrides.
#[test]
fn language_tag_settings_match_shared_cases() {
    let cases: Vec<(String, bool)> = serde_json::from_str(include_str!(
        "../../../shared-schema/language-tag-cases.json"
    ))
    .unwrap();
    for (tag, valid) in cases {
        for app_override in [false, true] {
            let mut config = AppConfig::default();
            let field = if app_override {
                config.correction.app_language_overrides = vec![format!("notepad.exe={tag}")];
                "correction.app_language_overrides"
            } else {
                config.correction.preferred_language = Some(tag.clone());
                "correction.preferred_language"
            };
            if valid {
                let encoded = config_to_toml(&config).unwrap();
                assert_eq!(
                    super::toml_io::parse_config(&encoded).unwrap().correction,
                    config.correction
                );
            } else {
                // App overrides intentionally trim the tag around '='.
                if app_override && tag.trim() != tag {
                    continue;
                }
                assert_eq!(config.validate().unwrap_err().field(), field, "{tag}");
                assert!(config_to_toml(&config).is_err(), "{tag}");
            }
        }
    }
}

#[test]
fn rejects_invalid_shortcut() {
    let mut config = AppConfig::default();
    config.shortcuts.correct = "Space".to_owned();

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "shortcuts.correct");
}

#[test]
fn rejects_conflicting_shortcuts() {
    let mut config = AppConfig::default();
    config.shortcuts.undo = config.shortcuts.correct.clone();

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "shortcuts.undo");
}

#[test]
fn rejects_streaming_correction() {
    let mut config = AppConfig::default();
    config.api.streaming = true;

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "api.streaming");
}

#[test]
fn custom_api_requires_safe_base_url() {
    let mut config = AppConfig::default();
    config.api.provider_preset = "custom".into();
    assert_eq!(config.validate().unwrap_err().field(), "api.base_url");
    config.api.base_url = Some("http://example.com/v1".into());
    assert_eq!(config.validate().unwrap_err().field(), "api.base_url");
    config.api.base_url = Some("http://127.0.0.1:9000/v1".into());
    assert!(config.validate().is_ok());
}

#[test]
fn rejects_zero_log_retention_days() {
    let mut config = AppConfig::default();
    config.logging.log_retention_days = Some(0);

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "logging.log_retention_days");
}

#[test]
fn saves_and_loads_config() {
    let path = std::env::temp_dir().join(format!(
        "autofix-config-{}.toml",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut config = AppConfig::default();
    config.general.start_with_windows = true;

    save_config(&path, &config).unwrap();
    let loaded = load_config(&path).unwrap();
    fs::remove_file(path).unwrap();

    assert_eq!(
        loaded.general.start_with_windows,
        config.general.start_with_windows
    );
}
