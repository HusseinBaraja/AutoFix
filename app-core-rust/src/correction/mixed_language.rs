//! Conservative edit boundaries for mixed-language executable text.

use super::{CorrectionInput, MixedLanguagePolicy};

pub(super) fn disabled(input: &CorrectionInput) -> bool {
    input.language_info.is_mixed()
        && input.mixed_language_policy == MixedLanguagePolicy::DisableCorrection
}

pub(super) fn edit_allowed(input: &CorrectionInput, start: usize, end: usize) -> bool {
    if !input.language_info.is_mixed() {
        return true;
    }
    if input.mixed_language_policy == MixedLanguagePolicy::DisableCorrection {
        return false;
    }
    let source = &input.executable_context[start..end];
    if source.is_empty() || !source.chars().all(char::is_alphabetic) {
        return false;
    }
    if input.mixed_language_policy == MixedLanguagePolicy::PerToken {
        return true;
    }
    let Some(primary) = input.language_info.primary_language.as_deref() else {
        return false;
    };
    let base = primary.split('-').next().unwrap_or("").to_ascii_lowercase();
    source.chars().all(|character| match base.as_str() {
        "en" | "fr" | "de" | "es" | "it" | "pt" | "nl" => is_latin(character),
        "ar" => (0x0600..=0x08ff).contains(&(character as u32)),
        "ru" => (0x0400..=0x052f).contains(&(character as u32)),
        "hi" => (0x0900..=0x097f).contains(&(character as u32)),
        "zh" => (0x4e00..=0x9fff).contains(&(character as u32)),
        "ja" => {
            (0x3040..=0x30ff).contains(&(character as u32))
                || (0x4e00..=0x9fff).contains(&(character as u32))
        }
        "ko" => (0xac00..=0xd7af).contains(&(character as u32)),
        "und" if primary.eq_ignore_ascii_case("und-Arab") => {
            (0x0600..=0x08ff).contains(&(character as u32))
        }
        "und" if primary.eq_ignore_ascii_case("und-Cyrl") => {
            (0x0400..=0x052f).contains(&(character as u32))
        }
        "und" if primary.eq_ignore_ascii_case("und-Deva") => {
            (0x0900..=0x097f).contains(&(character as u32))
        }
        "und" if primary.eq_ignore_ascii_case("und-Hani") => {
            (0x4e00..=0x9fff).contains(&(character as u32))
        }
        _ => false,
    })
}

pub(super) fn replacement_allowed(
    input: &CorrectionInput,
    start: usize,
    end: usize,
    replacement: &str,
) -> bool {
    if !input.language_info.is_mixed() {
        return true;
    }
    if !edit_allowed(input, start, end) {
        return false;
    }
    let original = &input.executable_context[start..end];
    // Preserve the script of each word. This also rejects translation and
    // multi-word rewrites in mixed text.
    let script = |character: char| match character as u32 {
        0x0600..=0x08ff => 1,
        0x0400..=0x052f => 2,
        0x0900..=0x097f => 3,
        0x4e00..=0x9fff => 4,
        0x3040..=0x30ff => 5,
        0xac00..=0xd7af => 6,
        _ if is_latin(character) => 0,
        _ => 7,
    };
    let original_script = original.chars().next().map(script);
    !replacement.is_empty()
        && replacement.chars().all(char::is_alphabetic)
        && replacement
            .chars()
            .all(|character| Some(script(character)) == original_script)
}

fn is_latin(character: char) -> bool {
    character.is_ascii_alphabetic()
        || (0x00c0..=0x024f).contains(&(character as u32))
        || (0x1e00..=0x1eff).contains(&(character as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::correction::{
        ConfidenceBehavior, ConfidenceBehaviorSettings, CorrectionMode, LanguageInfo, TriggerType,
        UncertainLanguagePolicy,
    };

    #[test]
    fn dominant_policy_keeps_foreign_spans_untouched() {
        let input = CorrectionInput {
            informative_context: String::new(),
            executable_context: "teh مرحبا".into(),
            mode: CorrectionMode::TyposOnly,
            enabled_grammar_categories: Vec::new(),
            language_info: LanguageInfo {
                primary_language: Some("en".into()),
                detected_languages: vec!["en".into(), "und-Arab".into()],
            },
            mixed_language_policy: MixedLanguagePolicy::DominantLanguageOnly,
            uncertain_language_policy: UncertainLanguagePolicy::default(),
            custom_dictionary: Vec::new(),
            protected_terms: Vec::new(),
            trigger_type: TriggerType::ManualShortcut,
            confidence_behavior: ConfidenceBehaviorSettings {
                high: ConfidenceBehavior::Silent,
                medium: ConfidenceBehavior::Suggestion,
                low: ConfidenceBehavior::DoNothing,
            },
        };
        assert!(edit_allowed(&input, 0, 3));
        assert!(!edit_allowed(&input, 4, input.executable_context.len()));
    }
}
