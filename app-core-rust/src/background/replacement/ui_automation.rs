//! UIA proves selection; keyboard input supplies mutation only at the document end.
//! TextPattern has no range setter. Never use ValuePattern.SetValue on whole fields.
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationTextRange,
    TextPatternRangeEndpoint_End as END, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
    UIA_IsReadOnlyAttributeId,
};

/// A selected-range keyboard adapter cannot prove a custom editor's overtype mode.
/// Require no newer following text and an exact document end before every mutation.
pub(super) fn authorize(
    element: &IUIAutomationElement,
    pattern: &IUIAutomationTextPattern,
    span: &IUIAutomationTextRange,
    following: &str,
) -> Result<(), String> {
    unsafe {
        let control = element
            .CurrentControlType()
            .map_err(|_| "control type unavailable")?;
        let document = pattern
            .DocumentRange()
            .map_err(|_| "document end unavailable")?;
        let at_end = span.CompareEndpoints(END, &document, END).ok() == Some(0);
        let read_only = read_only(span);
        admissible(
            control == UIA_EditControlTypeId || control == UIA_DocumentControlTypeId,
            read_only,
            at_end,
            following.is_empty(),
        )
    }
}

/// Native RichEdit can protect individual ranges independently of its HWND style.
/// Unknown/mixed attributes must not authorize any fallback mutation method.
pub(super) fn writable(span: &IUIAutomationTextRange) -> Result<(), String> {
    if read_only(span) == Some(false) {
        Ok(())
    } else {
        Err("selected range is protected or writability is unproved".into())
    }
}

fn read_only(span: &IUIAutomationTextRange) -> Option<bool> {
    unsafe { span.GetAttributeValue(UIA_IsReadOnlyAttributeId) }
        .ok()
        .and_then(|value| bool::try_from(&value).ok())
}

fn admissible(
    text_control: bool,
    read_only: Option<bool>,
    at_end: bool,
    no_following: bool,
) -> Result<(), String> {
    if !text_control || read_only != Some(false) {
        Err("UIA keyboard insertion requires a proved writable text range".into())
    } else if !at_end || !no_following {
        Err("UIA keyboard insertion cannot prove suffix-safe insert semantics".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_editor_overtype_cannot_consume_suffix_or_newer_typing() {
        assert!(admissible(true, Some(false), true, true).is_ok());
        assert!(admissible(true, Some(false), false, true).is_err());
        assert!(admissible(true, Some(false), true, false).is_err());
    }

    #[test]
    fn read_only_unknown_mixed_and_nontext_ranges_refuse() {
        for read_only in [Some(true), None] {
            assert!(admissible(true, read_only, true, true).is_err());
        }
        assert!(admissible(false, Some(false), true, true).is_err());
    }
}
