//! Read-only context at a newly observed caret position.

use crate::settings::ContextConfig;

use super::target::{CorrectionEligibility, FocusedTarget, TargetDetection};

/// Keep the nearest of the configured boundary, word limit, and text start.
pub(super) fn trim_before_caret<'a>(text: &'a str, limits: &ContextConfig) -> &'a str {
    let boundary_start = limits
        .initial_context_boundary_chars
        .iter()
        .filter(|boundary| !boundary.is_empty())
        .filter_map(|boundary| text.rfind(boundary).map(|index| index + boundary.len()))
        .max()
        .unwrap_or(0);

    let mut word_start = 0;
    let mut in_word = false;
    let mut words = 0;
    for (index, character) in text.char_indices().rev() {
        if character.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
                if words == usize::from(limits.initial_context_words) {
                    break;
                }
            }
        } else {
            if !in_word && words == usize::from(limits.initial_context_words) {
                break;
            }
            word_start = index;
            in_word = true;
        }
    }

    let total_words = words + usize::from(in_word);
    let word_start = if total_words < usize::from(limits.initial_context_words) {
        0
    } else {
        word_start
    };
    &text[boundary_start.max(word_start)..]
}

/// The batch may already have inserted executable typing before UIA is read.
/// Exclude that suffix so no character enters both contexts.
pub(super) fn captured_context(
    preceding: Option<&str>,
    executable: &str,
    limits: &ContextConfig,
) -> String {
    let Some(preceding) = preceding else {
        return String::new();
    };
    let preceding = if executable.is_empty() {
        preceding
    } else if let Some(prefix) = preceding.strip_suffix(executable) {
        prefix
    } else {
        return String::new();
    };
    trim_before_caret(preceding, limits).to_owned()
}

fn capture_char_limit(limits: &ContextConfig, known_typed_chars: usize) -> i32 {
    (limits.informative_context_max_chars as usize)
        .saturating_add(known_typed_chars)
        .min(i32::MAX as usize) as i32
}

#[cfg(windows)]
pub(super) fn read_before_caret(
    target: &FocusedTarget,
    limits: &ContextConfig,
    known_typed_chars: usize,
) -> Option<String> {
    use windows::Win32::{
        Foundation::{S_FALSE, S_OK},
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
        UI::Accessibility::{
            IUIAutomationTextPattern, TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
            TextUnit_Character, UIA_TextPatternId,
        },
    };

    if target.correction_eligibility() != CorrectionEligibility::Allowed
        || super::target::active_window_handle_value() != target.window_handle
    {
        return None;
    }
    unsafe {
        let initialization = CoInitializeEx(None, COINIT_MULTITHREADED);
        if initialization != S_OK && initialization != S_FALSE {
            return None;
        }
        let result = (|| {
            let automation = super::target::create_automation().ok()?;
            let element = automation.GetFocusedElement().ok()?;
            if element.CurrentIsPassword().ok()?.as_bool()
                || element.CurrentIsOffscreen().ok()?.as_bool()
                || !element.CurrentIsEnabled().ok()?.as_bool()
            {
                return None;
            }
            let pattern: IUIAutomationTextPattern =
                element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
            let selections = pattern.GetSelection().ok()?;
            if selections.Length().ok()? != 1 {
                return None;
            }
            let caret = selections.GetElement(0).ok()?;
            if caret
                .CompareEndpoints(
                    TextPatternRangeEndpoint_Start,
                    &caret,
                    TextPatternRangeEndpoint_End,
                )
                .ok()?
                != 0
            {
                return None;
            }
            let preceding = caret.Clone().ok()?;
            // The stored context is capped separately. Read enough to include
            // the known typed suffix, otherwise it can crowd the anchor out.
            let max_chars = capture_char_limit(limits, known_typed_chars);
            preceding
                .MoveEndpointByUnit(
                    TextPatternRangeEndpoint_Start,
                    TextUnit_Character,
                    -max_chars,
                )
                .ok()?;
            // GetText is bounded and the range ends at the collapsed caret.
            let text = preceding.GetText(max_chars).ok()?;
            let still_focused = automation.GetFocusedElement().ok()?;
            if !automation
                .CompareElements(&element, &still_focused)
                .ok()?
                .as_bool()
            {
                return None;
            }
            Some(text.to_string())
        })();
        CoUninitialize();
        // UIA calls cross process boundaries. Drop text if focus changed while
        // the range was being read.
        match super::target::detect_focused_target() {
            TargetDetection::Available(current)
                if current.process_id == target.process_id
                    && current.session_key() == target.session_key()
                    && current.correction_eligibility() == CorrectionEligibility::Allowed =>
            {
                result
            }
            _ => None,
        }
    }
}

#[cfg(not(windows))]
pub(super) fn read_before_caret(
    _target: &FocusedTarget,
    _limits: &ContextConfig,
    _known_typed_chars: usize,
) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_to_nearest_boundary_or_word_limit() {
        let limits = ContextConfig {
            initial_context_words: 3,
            ..ContextConfig::default()
        };
        assert_eq!(
            trim_before_caret("old. one two three four", &limits),
            "two three four"
        );
        assert_eq!(trim_before_caret("old. one two", &limits), " one two");
        assert_eq!(trim_before_caret("one two", &limits), "one two");
    }

    #[test]
    fn honors_multiple_boundaries_and_unicode_words() {
        let limits = ContextConfig {
            initial_context_words: 4,
            initial_context_boundary_chars: vec![".".into(), "؟".into()],
            ..ContextConfig::default()
        };
        assert_eq!(
            trim_before_caret("first. سابق؟ جديد هنا", &limits),
            " جديد هنا"
        );
    }

    #[test]
    fn excludes_executable_typing_and_fails_closed_on_unmatched_text() {
        let limits = ContextConfig::default();
        assert_eq!(captured_context(Some("Old. New"), "New", &limits), " ");
        assert_eq!(captured_context(Some("Old"), "New", &limits), "");
        assert_eq!(captured_context(None, "New", &limits), "");
    }

    #[test]
    fn capture_budget_includes_known_typing() {
        let limits = ContextConfig {
            informative_context_max_chars: 4,
            ..ContextConfig::default()
        };
        assert_eq!(capture_char_limit(&limits, 5), 9);
        assert_eq!(
            captured_context(Some("beforetyped"), "typed", &limits),
            "before"
        );
    }
}
