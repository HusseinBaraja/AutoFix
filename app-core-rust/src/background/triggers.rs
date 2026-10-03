//! Selects correction work from text known to have been typed in this run.

use super::{context_capture::SelectionCapture, security::TriggerKind, session::ContextVersions};
use crate::correction::{
    ConfidenceBehaviorSettings, CorrectionMode, EngineKind, LanguageInfo, MixedLanguagePolicy,
    UncertainLanguagePolicy,
};
use crate::settings::AppConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CorrectionRequest {
    pub(super) pending_segment_id: Option<u64>,
    /// Known typed text between a frozen range and the current caret. A native
    /// replacement owner must verify and preserve this text when targeting it.
    pub(super) replacement_following_text: String,
    pub(super) session_id: u64,
    pub(super) engine: EngineKind,
    pub(super) mode: CorrectionMode,
    pub(super) trigger: TriggerKind,
    pub(super) informative_context: String,
    pub(super) executable_context: String,
    pub(super) following_context: String,
    pub(super) selected_text: bool,
    pub(super) temporary_selection: bool,
    pub(super) versions: ContextVersions,
    pub(super) language_info: LanguageInfo,
    pub(super) uncertain_language_policy: UncertainLanguagePolicy,
    pub(super) mixed_language_policy: MixedLanguagePolicy,
    pub(super) confidence_behavior: ConfidenceBehaviorSettings,
}

/// Build a manual request only when its selected span is authorized.
pub(super) fn manual(
    session_id: u64,
    informative: &str,
    executable: &str,
    versions: ContextVersions,
    selection: &SelectionCapture,
    config: &AppConfig,
) -> Option<CorrectionRequest> {
    match selection {
        SelectionCapture::NoSelection => request(
            session_id,
            TriggerKind::ManualShortcut,
            informative,
            executable,
            versions,
            config,
        ),
        SelectionCapture::Selected {
            text,
            preceding,
            following,
            executable_prefix: Some(typed_before),
        } => {
            let context = format!("{informative}{typed_before}");
            let mut request = request(
                session_id,
                TriggerKind::ManualShortcut,
                &context,
                text,
                versions,
                config,
            )?;
            request.following_context = following.clone();
            request.selected_text = true;
            // The live capture is the proof of position; it is never editable.
            debug_assert!(preceding.ends_with(&context));
            Some(request)
        }
        SelectionCapture::Selected {
            text,
            preceding,
            following,
            executable_prefix: None,
        } if config.shortcuts.correct_arbitrary_selection => {
            let mut request = request(
                session_id,
                TriggerKind::ManualShortcut,
                preceding,
                text,
                versions,
                config,
            )?;
            request.following_context = following.clone();
            request.selected_text = true;
            request.temporary_selection = true;
            Some(request)
        }
        _ => None,
    }
}

/// Scope a configured automatic trigger to known text before the caret.
pub(super) fn automatic(
    session_id: u64,
    before: &str,
    after: &str,
    inserted: &str,
    informative: &str,
    versions: ContextVersions,
    config: &AppConfig,
) -> Option<CorrectionRequest> {
    if !config.correction.enabled || inserted.is_empty() || !after.ends_with(inserted) {
        return None;
    }
    if config.triggers.character_trigger_enabled
        && config
            .triggers
            .characters
            .iter()
            .filter(|boundary| !boundary.is_empty())
            .any(|boundary| after.ends_with(boundary.as_str()))
    {
        // A configured boundary can span multiple translated key events.
        // Freeze the complete active segment, including any earlier skipped
        // boundary, rather than dropping unchecked text from the request.
        return request(
            session_id,
            TriggerKind::Character,
            informative,
            after,
            versions,
            config,
        );
    }
    if config.triggers.word_count_enabled && config.triggers.word_count > 0 {
        let threshold = usize::from(config.triggers.word_count);
        let completed = |text: &str| {
            let words = text.split_whitespace().count();
            if text.chars().last().is_some_and(char::is_whitespace) {
                words
            } else {
                words.saturating_sub(1)
            }
        };
        let previous = completed(before);
        let current = completed(after);
        if current > previous && current / threshold > previous / threshold {
            return request(
                session_id,
                TriggerKind::WordCount,
                informative,
                after,
                versions,
                config,
            );
        }
    }
    None
}

/// Reject empty executable spans and initialize common request metadata.
fn request(
    session_id: u64,
    trigger: TriggerKind,
    informative: &str,
    executable: &str,
    versions: ContextVersions,
    config: &AppConfig,
) -> Option<CorrectionRequest> {
    (!executable.trim().is_empty()).then(|| CorrectionRequest {
        pending_segment_id: None,
        replacement_following_text: String::new(),
        session_id,
        engine: match config.correction.engine {
            crate::settings::CorrectionEngine::Local => EngineKind::LocalRule,
            crate::settings::CorrectionEngine::Api if config.api.provider_preset == "custom" => {
                EngineKind::CustomApi
            }
            crate::settings::CorrectionEngine::Api => EngineKind::OpenAiCompatibleApi,
        },
        mode: config.correction.mode,
        trigger,
        informative_context: informative.to_owned(),
        executable_context: executable.to_owned(),
        following_context: String::new(),
        selected_text: false,
        temporary_selection: false,
        versions,
        language_info: LanguageInfo {
            primary_language: None,
            detected_languages: Vec::new(),
        },
        uncertain_language_policy: UncertainLanguagePolicy::default(),
        mixed_language_policy: MixedLanguagePolicy::default(),
        confidence_behavior: config.confidence_behavior(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_trigger_snapshots_session_versions_engine_and_mode() {
        let versions = ContextVersions {
            context: 9,
            executable: 4,
            caret_anchor: 2,
        };
        let mut config = AppConfig::default();
        config.triggers.word_count = 1;
        config.correction.mode = CorrectionMode::TyposPlusGrammar;
        config.correction.engine = crate::settings::CorrectionEngine::Api;
        config.api.provider_preset = "custom".into();
        let requests = [
            manual(
                42,
                "read only",
                "teh",
                versions,
                &SelectionCapture::NoSelection,
                &config,
            )
            .unwrap(),
            automatic(42, "teh", "teh ", " ", "read only", versions, &config).unwrap(),
            automatic(42, "teh", "teh.", ".", "read only", versions, &config).unwrap(),
        ];
        config.correction.engine = crate::settings::CorrectionEngine::Local;
        config.correction.mode = CorrectionMode::TyposOnly;
        for request in requests {
            assert_eq!(request.session_id, 42);
            assert_eq!(request.versions, versions);
            assert_eq!(request.engine, EngineKind::CustomApi);
            assert_eq!(request.mode, CorrectionMode::TyposPlusGrammar);
        }
    }

    /// Known selections stay bounded; foreign selections need explicit opt-in.
    #[test]
    fn manual_uses_known_selected_suffix_or_prefix() {
        let v = ContextVersions::default();
        let mut config = AppConfig::default();
        let known = SelectionCapture::Selected {
            text: "ped".into(),
            preceding: "oldty".into(),
            following: " text later".into(),
            executable_prefix: Some("ty".into()),
        };
        let within = manual(1, "old", "typed text", v, &known, &config).unwrap();
        assert_eq!(within.executable_context, "ped");
        assert_eq!(within.informative_context, "oldty");
        assert_eq!(within.following_context, " text later");
        assert!(within.selected_text);
        assert!(!within.temporary_selection);
        let foreign = SelectionCapture::Selected {
            text: "foreign".into(),
            preceding: "elsewhere ".into(),
            following: " later".into(),
            executable_prefix: None,
        };
        assert!(manual(1, "old", "typed text", v, &foreign, &config).is_none());
        config.shortcuts.correct_arbitrary_selection = true;
        let arbitrary = manual(1, "old", "typed text", v, &foreign, &config).unwrap();
        assert_eq!(arbitrary.executable_context, "foreign");
        assert_eq!(arbitrary.informative_context, "elsewhere ");
        assert_eq!(arbitrary.following_context, " later");
        assert!(arbitrary.selected_text);
        assert!(arbitrary.temporary_selection);
        assert!(manual(
            1,
            "old",
            "typed text",
            v,
            &SelectionCapture::Unavailable,
            &config
        )
        .is_none());
        assert!(manual(1, "old", "", v, &SelectionCapture::NoSelection, &config).is_none());
    }

    /// Word thresholds and punctuation use their configured request scopes.
    #[test]
    fn automatic_uses_configured_threshold_and_completed_segment() {
        let mut config = AppConfig::default();
        config.triggers.word_count = 2;
        let v = ContextVersions::default();
        assert!(
            automatic(1, "one two", "one two ", " ", "old", v, &config).is_some_and(|r| r.trigger
                == TriggerKind::WordCount
                && r.informative_context == "old")
        );
        assert!(automatic(1, "one", "one ", " ", "", v, &config).is_none());
        assert_eq!(
            automatic(1, "First. Next", "First. Next.", ".", "", v, &config)
                .unwrap()
                .executable_context,
            "First. Next."
        );
        config.triggers.character_trigger_enabled = false;
        config.triggers.word_count_enabled = false;
        assert!(automatic(1, "One", "One.", ".", "", v, &config).is_none());
    }

    /// Character triggers exclude known text after the caret.
    #[test]
    fn configurable_character_and_caret_prefix_exclude_known_suffix() {
        use crate::background::{
            target::SessionKey,
            typing::{TypedInput, TypedSession},
        };
        let mut session = TypedSession::new();
        session.focus(Some((1, SessionKey::WindowHandle(1))));
        session.input(TypedInput::Text("First! Next? trailing".into()));
        for _ in 0..9 {
            session.input(TypedInput::Left);
        }
        let prefix = session.executable_context();
        assert_eq!(prefix, "First! Next?");
        let mut config = AppConfig::default();
        config.triggers.characters = vec!["!".into(), "?".into()];
        let request = automatic(
            1,
            "First! Next",
            &prefix,
            "?",
            "read only",
            ContextVersions::default(),
            &config,
        )
        .unwrap();
        assert_eq!(request.executable_context, "First! Next?");
        assert!(!request.executable_context.contains("trailing"));
    }
}
