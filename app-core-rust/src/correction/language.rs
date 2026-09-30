//! Language selection for a correction request. Detection never makes text editable.

use super::{LanguageInfo, UncertainLanguagePolicy};

/// Checks RFC 5646 structure, including extension payloads and private use.
/// Does not require subtags to be registered in the IANA registry.
pub(crate) fn valid_language_tag(tag: &str) -> bool {
    const GRANDFATHERED: &[&str] = &[
        "en-GB-oed",
        "i-ami",
        "i-bnn",
        "i-default",
        "i-enochian",
        "i-hak",
        "i-klingon",
        "i-lux",
        "i-mingo",
        "i-navajo",
        "i-pwn",
        "i-tao",
        "i-tay",
        "i-tsu",
        "sgn-BE-FR",
        "sgn-BE-NL",
        "sgn-CH-DE",
        "art-lojban",
        "cel-gaulish",
        "no-bok",
        "no-nyn",
        "zh-guoyu",
        "zh-hakka",
        "zh-min",
        "zh-min-nan",
        "zh-xiang",
    ];
    if GRANDFATHERED
        .iter()
        .any(|known| tag.eq_ignore_ascii_case(known))
    {
        return true;
    }
    let parts: Vec<&str> = tag.split('-').collect();
    if parts.iter().any(|part| {
        !(1..=8).contains(&part.len()) || !part.bytes().all(|byte| byte.is_ascii_alphanumeric())
    }) {
        return false;
    }
    let primary = parts[0];
    if primary.eq_ignore_ascii_case("x") {
        return parts.len() > 1;
    }
    let alphabetic = |part: &str| part.bytes().all(|byte| byte.is_ascii_alphabetic());
    if !(2..=8).contains(&primary.len()) || !alphabetic(primary) {
        return false;
    }
    let mut index = 1;
    if primary.len() <= 3 {
        for _ in 0..3 {
            if parts
                .get(index)
                .is_some_and(|part| part.len() == 3 && alphabetic(part))
            {
                index += 1;
            } else {
                break;
            }
        }
    }
    if parts
        .get(index)
        .is_some_and(|part| part.len() == 4 && alphabetic(part))
    {
        index += 1;
    }
    if parts.get(index).is_some_and(|part| {
        (part.len() == 2 && alphabetic(part))
            || (part.len() == 3 && part.bytes().all(|byte| byte.is_ascii_digit()))
    }) {
        index += 1;
    }
    let mut variants = std::collections::HashSet::new();
    while parts.get(index).is_some_and(|part| {
        part.len() >= 5 || (part.len() == 4 && part.as_bytes()[0].is_ascii_digit())
    }) {
        if !variants.insert(parts[index].to_ascii_lowercase()) {
            return false;
        }
        index += 1;
    }
    let mut extensions = std::collections::HashSet::new();
    while let Some(part) = parts.get(index) {
        if part.eq_ignore_ascii_case("x") {
            return index + 1 < parts.len();
        }
        if part.len() != 1 || !extensions.insert(part.to_ascii_lowercase()) {
            return false;
        }
        index += 1;
        let payload_start = index;
        while parts.get(index).is_some_and(|part| part.len() >= 2) {
            index += 1;
        }
        if index == payload_start {
            return false;
        }
    }
    true
}

/// Finds a validated app preference by case-insensitive process name.
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
    let current = if detected.len() == 1 {
        detected.first().cloned()
    } else if detected.len() > 1 {
        let text = format!("{informative} {executable}");
        detected
            .iter()
            .max_by_key(|tag| {
                text.chars()
                    .filter(|character| match tag.as_str() {
                        "en" => character.is_ascii_alphabetic(),
                        "und-Arab" => (0x0600..=0x08ff).contains(&(*character as u32)),
                        "und-Cyrl" => (0x0400..=0x052f).contains(&(*character as u32)),
                        "und-Deva" => (0x0900..=0x097f).contains(&(*character as u32)),
                        "und-Hani" => (0x4e00..=0x9fff).contains(&(*character as u32)),
                        "ja" => (0x3040..=0x30ff).contains(&(*character as u32)),
                        "ko" => (0xac00..=0xd7af).contains(&(*character as u32)),
                        _ => false,
                    })
                    .count()
            })
            .cloned()
    } else {
        None
    };
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
/// English needs at least two matches comprising 15% of all Unicode words.
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
    let mut word_count = 0usize;
    let mut matches = 0usize;
    for word in text
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
    {
        word_count += 1;
        if english_words
            .iter()
            .any(|known| word.eq_ignore_ascii_case(known))
        {
            matches += 1;
        }
    }
    if matches >= 2 && matches * 100 >= word_count * 15 {
        result.push("en".to_owned());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// English quotes in mostly foreign Latin text must retain the uncertainty guard.
    #[test]
    fn sparse_english_function_words_leave_latin_text_unknown() {
        let informative = "école élève été forêt où êtes île hôtel âge cœur façade Noël dîner";
        let selection = resolve(
            informative,
            "for you",
            None,
            None,
            None,
            UncertainLanguagePolicy::default(),
        );
        assert!(selection.info.detected_languages.is_empty());
        assert!(selection.info.is_uncertain());
        assert_eq!(selection.session_detected, None);
    }

    /// Both the match count and inclusive 15% ratio must hold across both contexts.
    #[test]
    fn english_detection_requires_two_matches_and_fifteen_percent() {
        assert!(detect("the", "word word word").is_empty());
        let twelve_words = "été été été été été été été été été été été été";
        assert!(detect(twelve_words, "the and").is_empty());
        let seventeen_words = "été été été été été été été été été été été été été été été été été";
        assert_eq!(detect(seventeen_words, "THE AND YOU"), ["en"]);
        assert_eq!(detect("This is the context.", "teh word"), ["en"]);
    }

    /// Rust and settings UI use the same accepted and rejected language-tag corpus.
    #[test]
    fn language_tags_match_shared_cases() {
        let cases: Vec<(String, bool)> = serde_json::from_str(include_str!(
            "../../../shared-schema/language-tag-cases.json"
        ))
        .unwrap();
        for (tag, expected) in cases {
            assert_eq!(valid_language_tag(&tag), expected, "{tag}");
        }
    }

    /// Detection uses both contexts while app preferences take precedence over global settings.
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

    /// Unknown text remains uncertain despite cached English; mixed scripts retain both detections.
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
        assert_eq!(mixed.info.primary_language.as_deref(), Some("en"));
    }

    /// App language preferences match process names regardless of letter case.
    #[test]
    fn app_override_matches_process_case_insensitively() {
        let entries = vec!["Notepad.exe=fr-FR".to_owned()];
        assert_eq!(app_override(&entries, "notepad.exe"), Some("fr-FR"));
        assert_eq!(app_override(&entries, "word.exe"), None);
    }
}
