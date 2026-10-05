//! Resolve a keyboard-owned editor when UIA reports an opaque native host.
use windows::Win32::UI::Accessibility::{
    IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern, TreeScope_Descendants,
    UIA_HasKeyboardFocusPropertyId, UIA_TextPatternId,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, IsChild, GUITHREADINFO,
};

const MAX_FOCUSED_REFERENCES: i32 = 256;

fn refusal(reason: &'static str) -> windows::core::Error {
    tracing::debug!(stage = reason, "focused text resolution refused");
    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), reason)
}

fn candidate_safe(properties: [Option<bool>; 4], same_process: bool) -> bool {
    same_process && properties == [Some(false), Some(false), Some(true), Some(true)]
}

/// Runtime identity alone is insufficient: providers can reuse IDs for different elements.
fn unique_editor<T>(
    candidates: impl IntoIterator<Item = (T, Option<String>)>,
    mut same_element: impl FnMut(&T, &T) -> windows::core::Result<bool>,
) -> windows::core::Result<T> {
    let mut selected: Option<(T, String)> = None;
    for (candidate, identity) in candidates {
        let identity = identity
            .filter(|id| !id.is_empty())
            .ok_or_else(|| refusal("editor identity unavailable"))?;
        if let Some((previous, previous_id)) = &selected {
            if identity != *previous_id || !same_element(previous, &candidate)? {
                return Err(refusal("distinct focused editors are ambiguous"));
            }
        } else {
            selected = Some((candidate, identity));
        }
    }
    selected
        .map(|(element, _)| element)
        .ok_or_else(|| refusal("no focused text editor"))
}

/// Every caller uses the same resolution for admission, capture and mutation.
/// Never choose an editor by its text, name, position, or the first matching reference.
pub(in crate::background) fn resolve(
    automation: &IUIAutomation2,
) -> windows::core::Result<IUIAutomationElement> {
    unsafe {
        let foreground = GetForegroundWindow();
        let mut process = 0;
        let thread = GetWindowThreadProcessId(foreground, &mut process);
        let mut keyboard: GUITHREADINFO = std::mem::zeroed();
        keyboard.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if foreground.is_null()
            || process == 0
            || thread == 0
            || GetGUIThreadInfo(thread, &mut keyboard) == 0
            || keyboard.hwndFocus.is_null()
        {
            return Err(refusal("keyboard ownership unavailable"));
        }
        let mut keyboard_process = 0;
        GetWindowThreadProcessId(keyboard.hwndFocus, &mut keyboard_process);
        if keyboard_process != process {
            return Err(refusal("keyboard owner differs from foreground process"));
        }
        let host = automation.GetFocusedElement()?;
        if host.CurrentProcessId()? as u32 != process {
            return Err(refusal("focused provider differs from foreground process"));
        }
        // An explicit password always reaches the security block, never a descendant search.
        if host.CurrentIsPassword()?.as_bool() {
            return Ok(host);
        }
        if host
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .is_ok()
        {
            if !host.CurrentHasKeyboardFocus()?.as_bool() {
                return Err(refusal("text provider does not report keyboard focus"));
            }
            if GetForegroundWindow() != foreground {
                return Err(refusal("foreground changed during resolution"));
            }
            return Ok(host);
        }
        if host.CurrentIsOffscreen()?.as_bool() || !host.CurrentIsEnabled()?.as_bool() {
            return Err(refusal("opaque host is unavailable"));
        }
        let host_window = host.CurrentNativeWindowHandle()?.0;
        if host_window.is_null()
            || (host_window != keyboard.hwndFocus && IsChild(host_window, keyboard.hwndFocus) == 0)
        {
            return Err(refusal("opaque host does not own keyboard focus"));
        }
        let condition = automation.CreatePropertyCondition(
            UIA_HasKeyboardFocusPropertyId,
            &windows::Win32::System::Variant::VARIANT::from(true),
        )?;
        let references = host.FindAll(TreeScope_Descendants, &condition)?;
        let count = references.Length()?;
        if count > MAX_FOCUSED_REFERENCES {
            return Err(refusal("focused editor reference budget exceeded"));
        }
        tracing::debug!(focused_references = count, "resolving opaque focused host");
        let mut candidates = Vec::new();
        for index in 0..count {
            let element = references.GetElement(index)?;
            if !element.CurrentHasKeyboardFocus()?.as_bool() {
                return Err(refusal("descendant focus changed during resolution"));
            }
            if element.CurrentIsPassword()?.as_bool() {
                return Err(refusal("focused descendant is protected"));
            }
            if element.CurrentProcessId()? as u32 != process {
                return Err(refusal("focused descendant has a different owner"));
            }
            if element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .is_ok()
            {
                if !candidate_safe(
                    [
                        element
                            .CurrentIsPassword()
                            .ok()
                            .map(|value| value.as_bool()),
                        element
                            .CurrentIsOffscreen()
                            .ok()
                            .map(|value| value.as_bool()),
                        element.CurrentIsEnabled().ok().map(|value| value.as_bool()),
                        element
                            .CurrentHasKeyboardFocus()
                            .ok()
                            .map(|value| value.as_bool()),
                    ],
                    element.CurrentProcessId()? as u32 == process,
                ) {
                    return Err(refusal("focused editor safety is unproved"));
                }
                let identity = super::runtime_id(&element);
                candidates.push((element, identity));
            }
        }
        let editor = unique_editor(candidates, |left, right| {
            automation
                .CompareElements(left, right)
                .map(|equal| equal.as_bool())
        })?;
        if GetForegroundWindow() != foreground
            || !automation
                .CompareElements(&host, &automation.GetFocusedElement()?)?
                .as_bool()
        {
            return Err(refusal("focused host changed during resolution"));
        }
        let mut still_keyboard: GUITHREADINFO = std::mem::zeroed();
        still_keyboard.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(thread, &mut still_keyboard) == 0
            || still_keyboard.hwndFocus != keyboard.hwndFocus
        {
            return Err(refusal("keyboard owner changed during resolution"));
        }
        Ok(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hidden_disabled_unfocused_foreign_and_unknown_editors_refuse() {
        let safe = [Some(false), Some(false), Some(true), Some(true)];
        assert!(candidate_safe(safe, true));
        assert!(!candidate_safe(safe, false));
        for index in 0..safe.len() {
            let mut unknown = safe;
            unknown[index] = None;
            assert!(!candidate_safe(unknown, true));
            let mut unsafe_field = safe;
            unsafe_field[index] = safe[index].map(|value| !value);
            assert!(!candidate_safe(unsafe_field, true));
        }
    }

    #[test]
    fn repeated_references_to_one_editor_are_not_multiple_fields() {
        let references = (0..163).map(|_| (7, Some("editor".into())));
        assert_eq!(unique_editor(references, |a, b| Ok(a == b)).unwrap(), 7);
    }

    #[test]
    fn collisions_distinct_editors_and_unknown_comparison_refuse() {
        for second in [
            (8, Some("editor".into())),
            (7, Some("other".into())),
            (7, None),
        ] {
            assert!(
                unique_editor([(7, Some("editor".into())), second], |a, b| Ok(a == b)).is_err()
            );
        }
        assert!(unique_editor(
            [(7, Some("editor".into())), (7, Some("editor".into()))],
            |_, _| Err(refusal("comparison unavailable"))
        )
        .is_err());
        assert!(unique_editor::<i32>([], |_, _| Ok(true)).is_err());
    }
}
