//! Selects correction work from text known to have been typed in this run.

use super::{security::TriggerKind, session::ContextVersions};
use crate::settings::AppConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CorrectionRequest {
    pub(super) trigger: TriggerKind,
    pub(super) informative_context: String,
    pub(super) executable_context: String,
    pub(super) versions: ContextVersions,
}

pub(super) fn manual(
    informative: &str,
    executable: &str,
    versions: ContextVersions,
    selected: Option<&str>,
) -> Option<CorrectionRequest> {
    // A selection is executable only when it is a known suffix ending at the
    // tracked caret. A foreign or unverified selection falls back to the prefix.
    let scope = selected
        .filter(|text| !text.is_empty() && executable.ends_with(text))
        .unwrap_or(executable);
    request(TriggerKind::ManualShortcut, informative, scope, versions)
}

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
        versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_uses_known_selected_suffix_or_prefix() {
        let v = ContextVersions::default();
        assert_eq!(
            manual("old", "typed text", v, Some("text"))
                .unwrap()
                .executable_context,
            "text"
        );
        assert_eq!(
            manual("old", "typed text", v, Some("foreign"))
                .unwrap()
                .executable_context,
            "typed text"
        );
        assert!(manual("old", "", v, None).is_none());
    }

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
