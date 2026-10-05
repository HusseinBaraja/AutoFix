//! UIA proves selection; use one selected character or a proved field end.
//! TextPattern has no range setter. Never use ValuePattern.SetValue on whole fields.
use windows::Win32::UI::Accessibility::{
    IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationTextRange,
    TextPatternRangeEndpoint_End as END, TextPatternRangeEndpoint_Start as START,
    UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_IsReadOnlyAttributeId,
};

/// A selected-range keyboard adapter cannot prove a custom editor's overtype mode.
/// Multi-character input needs a proved field end and no newer following text.
pub(super) fn authorize(
    automation: &IUIAutomation2,
    element: &IUIAutomationElement,
    pattern: &IUIAutomationTextPattern,
    span: &IUIAutomationTextRange,
    following: &str,
    original: &str,
    replacement: &str,
) -> Result<(), String> {
    unsafe {
        let control = element
            .CurrentControlType()
            .map_err(|_| "control type unavailable")?;
        let span_read_only = read_only(span);
        // One BMP character replaces a nonempty proved selection in one key
        // operation. Even overtype mode cannot consume an unselected suffix.
        // Deletion similarly sends one Backspace for the verified selection.
        if atomic_selected_edit(original, replacement) {
            return admissible(
                control == UIA_EditControlTypeId || control == UIA_DocumentControlTypeId,
                span_read_only,
                true,
                true,
            );
        }
        let document = pattern
            .DocumentRange()
            .map_err(|_| "document end unavailable")?;
        let at_document_end = span.CompareEndpoints(END, &document, END).ok() == Some(0);
        // Hosted editors may share a provider with surrounding page text. Require
        // an independently enclosed writable Edit range, never a guessed suffix.
        let field = if !at_document_end && control == UIA_EditControlTypeId {
            pattern.RangeFromChild(element).ok()
        } else {
            None
        };
        let field_same_owner = field
            .as_ref()
            .and_then(|field| field.GetEnclosingElement().ok())
            .map(|owner| owned_by_editor(automation, element, &owner));
        let field_contains_start = field
            .as_ref()
            .and_then(|field| span.CompareEndpoints(START, field, START).ok())
            .map(|order| order >= 0);
        let at_field_end = field.as_ref().and_then(|field| {
            let order = span.CompareEndpoints(END, field, END).ok()?;
            if order < 0 {
                return Some(false);
            }
            // Chromium distinguishes the final text-node endpoint from the
            // collapsed caret immediately after it. Accept that empty gap only
            // when both the selected span and its end remain inside this Edit.
            let gap = field.Clone().ok()?;
            gap.MoveEndpointByRange(START, field, END).ok()?;
            gap.MoveEndpointByRange(END, span, END).ok()?;
            let end = span.Clone().ok()?;
            end.MoveEndpointByRange(START, span, END).ok()?;
            Some(
                gap.GetText(1).ok()?.is_empty()
                    && gap.GetChildren().ok()?.Length().ok()? == 0
                    && owned_by_editor(automation, element, &span.GetEnclosingElement().ok()?)
                    && owned_by_editor(automation, element, &end.GetEnclosingElement().ok()?),
            )
        });
        let field_writable = field.as_ref().and_then(read_only);
        let field_end_proved = scoped_field_end([
            field_same_owner,
            field_contains_start,
            at_field_end,
            field_writable.map(|value| !value),
        ]);
        tracing::debug!(
            at_document_end,
            field_available = field.is_some(),
            field_same_owner,
            field_contains_start,
            at_field_end,
            field_writable,
            "UIA insertion boundary checked"
        );
        admissible(
            control == UIA_EditControlTypeId || control == UIA_DocumentControlTypeId,
            span_read_only,
            at_document_end || field_end_proved,
            following.is_empty(),
        )
    }
}

pub(super) fn atomic_selected_edit(original: &str, replacement: &str) -> bool {
    !original.is_empty()
        && replacement.encode_utf16().count() <= 1
        && !replacement.chars().any(char::is_control)
}

/// RangeFromChild may enclose an inner text node with the same range. Require
/// its live ancestry to reach this exact Edit without crossing another editor.
unsafe fn owned_by_editor(
    automation: &IUIAutomation2,
    editor: &IUIAutomationElement,
    child: &IUIAutomationElement,
) -> bool {
    let Ok(walker) = automation.RawViewWalker() else {
        return false;
    };
    let mut node = child.clone();
    for _ in 0..32 {
        if automation
            .CompareElements(editor, &node)
            .is_ok_and(|same| same.as_bool())
        {
            return true;
        }
        let Ok(control) = node.CurrentControlType() else {
            return false;
        };
        if control == UIA_EditControlTypeId || control == UIA_DocumentControlTypeId {
            return false;
        }
        let Ok(parent) = walker.GetParentElement(&node) else {
            return false;
        };
        node = parent;
    }
    false
}

fn scoped_field_end(proof: [Option<bool>; 4]) -> bool {
    proof == [Some(true); 4]
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
    fn embedded_edit_boundary_needs_exact_owner_containment_end_and_writability() {
        assert!(scoped_field_end([Some(true); 4]));
        for index in 0..4 {
            for missing in [None, Some(false)] {
                let mut proof = [Some(true); 4];
                proof[index] = missing;
                assert!(!scoped_field_end(proof));
            }
        }
    }

    #[test]
    fn atomic_selected_edit_cannot_insert_at_empty_selection_or_type_multiple_units() {
        assert!(atomic_selected_edit("eh", "h"));
        assert!(atomic_selected_edit("e", ""));
        assert!(!atomic_selected_edit("", "h"));
        for replacement in ["he", "😃", "\n", "\0"] {
            assert!(!atomic_selected_edit("e", replacement));
        }
    }

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
