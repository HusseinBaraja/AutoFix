//! Language selection for a correction request. Detection never makes text editable.

use super::{LanguageInfo, UncertainLanguagePolicy};

pub(crate) fn valid_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    (2..=8).contains(&primary.len())
        && primary.bytes().all(|byte| byte.is_ascii_alphabetic())
        && parts.all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

pub(crate) fn app_override<'a>(entries: &'a [String], process_name: &str) -> Option<&'a str> {
    entries.iter().find_map(|entry| {
        let (app, tag) = entry.split_once('=')?;
        app.trim()
            .eq_ignore_ascii_case(process_name)
            .then(|| tag.trim())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanguageSelection {
    pub(crate) info: LanguageInfo,
    pub(crate) policy: UncertainLanguagePolicy,
    pub(crate) session_detected: Option<String>,
}

/// App and global preferences select the primary language. Detection still
/// determines whether the current text is uncertain or mixed.
pub(crate) fn resolve(
    informative: &str,
    executable: &str,
    session_detected: Option<&str>,
    app_override: Option<&str>,
    global_preferred: Option<&str>,
    policy: UncertainLanguagePolicy,
) -> LanguageSelection {
    let detected = detect(informative, executable);
    let current = (detected.len() == 1)
        .then(|| detected.first().cloned())
        .flatten();
    let session_detected = current.or_else(|| session_detected.map(str::to_owned));
    let primary = app_override
        .or(global_preferred)
        .map(str::to_owned)
        .or_else(|| session_detected.clone());
    LanguageSelection {
        info: LanguageInfo {
            primary_language: primary,
            detected_languages: detected.clone(),
        },
        policy,
        session_detected,
    }
}

/// Conservative detector: recognize clear scripts and English function words.
/// Other Latin text remains unknown until a language detector is added.
fn detect(informative: &str, executable: &str) -> Vec<String> {
    let text = format!("{informative} {executable}");
    let mut result = Vec::new();
    let scripts = [
        ("und-Arab", 0x0600, 0x08ff),
        ("und-Cyrl", 0x0400, 0x052f),
        ("und-Deva", 0x0900, 0x097f),
        ("und-Hani", 0x4e00, 0x9fff),
        ("ja", 0x3040, 0x30ff),
        ("ko", 0xac00, 0xd7af),
    ];
    for (tag, start, end) in scripts {
        if text.chars().any(|c| (start..=end).contains(&(c as u32))) {
            result.push(tag.to_owned());
        }
    }
    let english_words = [
        "the", "and", "that", "this", "with", "from", "have", "will", "are", "you", "for", "not",
        "was", "but",
    ];
    let matches = text
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| {
            english_words
                .iter()
                .any(|known| word.eq_ignore_ascii_case(known))
        })
        .count();
    if matches >= 2 {
        result.push("en".to_owned());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_from_both_contexts_and_uses_override_precedence() {
        let selection = resolve(
            "This is the context. ",
            "teh word",
            None,
            Some("fr-CA"),
            Some("en-US"),
            UncertainLanguagePolicy::default(),
        );
        assert_eq!(selection.info.detected_languages, ["en"]);
        assert_eq!(selection.info.primary_language.as_deref(), Some("fr-CA"));
        assert_eq!(selection.session_detected.as_deref(), Some("en"));
        assert!(selection.info.is_uncertain());
        let automatic = resolve(
            "This is the context. ",
            "teh word",
            None,
            None,
            None,
            UncertainLanguagePolicy::default(),
        );
        assert_eq!(automatic.info.primary_language.as_deref(), Some("en"));
        assert!(!automatic.info.is_uncertain());
    }

    #[test]
    fn unknown_and_mixed_text_are_uncertain() {
        let unknown = resolve(
            "",
            "teh",
            Some("en"),
            None,
            None,
            UncertainLanguagePolicy::default(),
        );
        assert!(unknown.info.is_uncertain());
        assert_eq!(unknown.info.primary_language.as_deref(), Some("en"));
        let mixed = resolve(
            "the and ",
            "مرحبا",
            None,
            None,
            None,
            UncertainLanguagePolicy::default(),
        );
        assert!(mixed.info.is_uncertain());
        assert!(mixed
            .info
            .detected_languages
            .contains(&"und-Arab".to_owned()));
        assert!(mixed.info.detected_languages.contains(&"en".to_owned()));
    }

    #[test]
    fn app_override_matches_process_case_insensitively() {
        let entries = vec!["Notepad.exe=fr-FR".to_owned()];
        assert_eq!(app_override(&entries, "notepad.exe"), Some("fr-FR"));
        assert_eq!(app_override(&entries, "word.exe"), None);
    }
}
