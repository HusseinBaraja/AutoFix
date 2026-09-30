//! Selects correction work from text known to have been typed in this run.

use super::{context_capture::SelectionCapture, security::TriggerKind, session::ContextVersions};
use crate::correction::{
    ConfidenceBehaviorSettings, LanguageInfo, MixedLanguagePolicy, UncertainLanguagePolicy,
};
use crate::settings::AppConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CorrectionRequest {
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
    informative: &str,
    executable: &str,
    versions: ContextVersions,
    selection: &SelectionCapture,
    allow_arbitrary_selection: bool,
) -> Option<CorrectionRequest> {
    match selection {
        SelectionCapture::NoSelection => request(
            TriggerKind::ManualShortcut,
            informative,
            executable,
            versions,
        ),
        SelectionCapture::Selected {
            text,
            preceding,
            following,
            executable_prefix: Some(typed_before),
        } => {
            let context = format!("{informative}{typed_before}");
            let mut request = request(TriggerKind::ManualShortcut, &context, text, versions)?;
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
        } if allow_arbitrary_selection => {
            let mut request = request(TriggerKind::ManualShortcut, preceding, text, versions)?;
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
    before: &str,
    after: &str,
    inserted: &str,
    informative: &str,
    versions: ContextVersions,
    config: &AppConfig,
) -> Option<CorrectionRequest> {
    if inserted.is_empty() || !after.ends_with(inserted) {
        return None;
    }
    if config.triggers.character_trigger_enabled {
        if let Some(boundary) = config
            .triggers
            .characters
            .iter()
            .filter(|boundary| !boundary.is_empty())
            .find(|boundary| inserted.ends_with(boundary.as_str()))
        {
            let preceding = &after[..after.len() - boundary.len()];
            let start = config
                .triggers
                .characters
                .iter()
                .filter(|character| !character.is_empty())
                .filter_map(|character| preceding.rfind(character).map(|at| at + character.len()))
                .max()
                .unwrap_or(0);
            return request(
                TriggerKind::Character,
                informative,
                &after[start..],
                versions,
            );
        }
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
            return request(TriggerKind::WordCount, informative, after, versions);
        }
    }
    None
}

/// Reject empty executable spans and initialize common request metadata.
fn request(
    trigger: TriggerKind,
    informative: &str,
    executable: &str,
    versions: ContextVersions,
) -> Option<CorrectionRequest> {
    (!executable.trim().is_empty()).then(|| CorrectionRequest {
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
        confidence_behavior: ConfidenceBehaviorSettings::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known selections stay bounded; foreign selections need explicit opt-in.
    #[test]
    fn manual_uses_known_selected_suffix_or_prefix() {
        let v = ContextVersions::default();
        let known = SelectionCapture::Selected {
            text: "ped".into(),
            preceding: "oldty".into(),
            following: " text later".into(),
            executable_prefix: Some("ty".into()),
        };
        let within = manual("old", "typed text", v, &known, false).unwrap();
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
        assert!(manual("old", "typed text", v, &foreign, false).is_none());
        let arbitrary = manual("old", "typed text", v, &foreign, true).unwrap();
        assert_eq!(arbitrary.executable_context, "foreign");
        assert_eq!(arbitrary.informative_context, "elsewhere ");
        assert_eq!(arbitrary.following_context, " later");
        assert!(arbitrary.selected_text);
        assert!(arbitrary.temporary_selection);
        assert!(manual("old", "typed text", v, &SelectionCapture::Unavailable, true).is_none());
        assert!(manual("old", "", v, &SelectionCapture::NoSelection, false).is_none());
    }

    /// Word thresholds and punctuation use their configured request scopes.
    #[test]
    fn automatic_uses_configured_threshold_and_completed_segment() {
        let mut config = AppConfig::default();
        config.triggers.word_count = 2;
        let v = ContextVersions::default();
        assert!(
            automatic("one two", "one two ", " ", "old", v, &config).is_some_and(|r| r.trigger
                == TriggerKind::WordCount
                && r.informative_context == "old")
        );
        assert!(automatic("one", "one ", " ", "", v, &config).is_none());
        assert_eq!(
            automatic("First. Next", "First. Next.", ".", "", v, &config)
                .unwrap()
                .executable_context,
            " Next."
        );
        config.triggers.character_trigger_enabled = false;
        config.triggers.word_count_enabled = false;
        assert!(automatic("One", "One.", ".", "", v, &config).is_none());
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
            "First! Next",
            &prefix,
            "?",
            "read only",
            ContextVersions::default(),
            &config,
        )
        .unwrap();
        assert_eq!(request.executable_context, " Next?");
        assert!(!request.executable_context.contains("trailing"));
    }
}
