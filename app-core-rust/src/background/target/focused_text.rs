//! Resolve a keyboard-owned editor when UIA reports an opaque native host.
use windows::Win32::System::Variant::{VARIANT, VT_ARRAY, VT_I4};
use windows::Win32::UI::Accessibility::{
    IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern, TreeScope_Descendants,
    UIA_ControlTypePropertyId, UIA_HasKeyboardFocusPropertyId, UIA_IsEnabledPropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId, UIA_NativeWindowHandlePropertyId,
    UIA_ProcessIdPropertyId, UIA_RuntimeIdPropertyId, UIA_TextPatternId,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, IsChild, GUITHREADINFO,
};

const MAX_FOCUSED_REFERENCES: i32 = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ResolutionKey {
    window: isize,
    keyboard: isize,
    process: u32,
    position: u64,
    sequence: u64,
}

#[derive(Clone)]
struct ResolvedEditor {
    key: ResolutionKey,
    host: IUIAutomationElement,
    editor: IUIAutomationElement,
    identity: String,
}

thread_local! {
    static RESOLUTION_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static RESOLVED_EDITOR: std::cell::RefCell<Option<ResolvedEditor>> = const { std::cell::RefCell::new(None) };
}

/// Keep COM alive and editor discovery local to one guarded input operation.
/// Cached discovery is never reused across work items or input/focus changes.
pub(in crate::background) struct ResolutionScope(bool, std::marker::PhantomData<std::rc::Rc<()>>);
impl ResolutionScope {
    pub(in crate::background) fn begin() -> Self {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let initialized =
            super::accept_com_initialization(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) });
        if initialized {
            RESOLUTION_DEPTH.with(|depth| depth.set(depth.get() + 1));
        }
        Self(initialized, std::marker::PhantomData)
    }
}
impl Drop for ResolutionScope {
    fn drop(&mut self) {
        if self.0 {
            RESOLUTION_DEPTH.with(|depth| {
                depth.set(depth.get() - 1);
                if depth.get() == 0 {
                    RESOLVED_EDITOR.with(|editor| editor.borrow_mut().take());
                }
            });
            // Release every stored COM reference before releasing this apartment.
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
}

fn refusal(reason: &'static str) -> windows::core::Error {
    tracing::debug!(stage = reason, "focused text resolution refused");
    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), reason)
}

fn candidate_safe(properties: [Option<bool>; 4], same_process: bool) -> bool {
    same_process && properties == [Some(false), Some(false), Some(true), Some(true)]
}

fn cached_identity(element: &IUIAutomationElement) -> Option<String> {
    unsafe {
        let value = element
            .GetCachedPropertyValue(UIA_RuntimeIdPropertyId)
            .ok()?;
        identity_from_variant(&value)
    }
}

fn identity_from_variant(value: &VARIANT) -> Option<String> {
    unsafe {
        let raw = &value.Anonymous.Anonymous;
        if raw.vt != (VT_ARRAY | VT_I4) {
            return None;
        }
        // The VARIANT owns this array; do not destroy it independently.
        let values = super::safe_array_i32_values(raw.Anonymous.parray)?;
        (!values.is_empty()).then(|| {
            values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(".")
        })
    }
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
        if !super::provider_owner_matches(process, host.CurrentProcessId()? as u32) {
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
        let key = ResolutionKey {
            window: foreground as isize,
            keyboard: keyboard.hwndFocus as isize,
            process,
            position: super::super::input_listener::current_position_generation(),
            sequence: super::super::input_listener::current_input_sequence(),
        };
        let previous = RESOLVED_EDITOR.with(|editor| editor.borrow().clone());
        if let Some(previous) = previous.filter(|previous| previous.key == key) {
            // Recheck native ownership, the current UIA host and all live field
            // properties. Only the duplicate-reference discovery step is reused.
            if automation.CompareElements(&previous.host, &host)?.as_bool()
                && candidate_safe(
                    [
                        previous
                            .editor
                            .CurrentIsPassword()
                            .ok()
                            .map(|value| value.as_bool()),
                        previous
                            .editor
                            .CurrentIsOffscreen()
                            .ok()
                            .map(|value| value.as_bool()),
                        previous
                            .editor
                            .CurrentIsEnabled()
                            .ok()
                            .map(|value| value.as_bool()),
                        previous
                            .editor
                            .CurrentHasKeyboardFocus()
                            .ok()
                            .map(|value| value.as_bool()),
                    ],
                    super::provider_owner_matches(
                        process,
                        previous.editor.CurrentProcessId()? as u32,
                    ),
                )
                && super::runtime_id(&previous.editor).as_deref()
                    == Some(previous.identity.as_str())
                && GetForegroundWindow() == foreground
                && super::super::input_listener::current_position_generation() == key.position
                && super::super::input_listener::current_input_sequence() == key.sequence
            {
                let mut still_keyboard: GUITHREADINFO = std::mem::zeroed();
                still_keyboard.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
                if GetGUIThreadInfo(thread, &mut still_keyboard) == 0
                    || still_keyboard.hwndFocus != keyboard.hwndFocus
                {
                    return Err(refusal("keyboard owner changed during resolution"));
                }
                tracing::debug!("focused editor discovery reused after live validation");
                return Ok(previous.editor);
            }
        }
        RESOLVED_EDITOR.with(|editor| editor.borrow_mut().take());
        let condition = automation.CreatePropertyCondition(
            UIA_HasKeyboardFocusPropertyId,
            &windows::Win32::System::Variant::VARIANT::from(true),
        )?;
        // One fresh bulk snapshot per guarded discovery, never a persisted authorization cache.
        // Per-reference COM property calls turn duplicate providers into input backlog.
        let resolution_started = std::time::Instant::now();
        let cache = automation.CreateCacheRequest()?;
        for property in [
            UIA_ProcessIdPropertyId,
            UIA_HasKeyboardFocusPropertyId,
            UIA_IsPasswordPropertyId,
            UIA_IsOffscreenPropertyId,
            UIA_IsEnabledPropertyId,
            UIA_RuntimeIdPropertyId,
            UIA_NativeWindowHandlePropertyId,
            UIA_ControlTypePropertyId,
        ] {
            cache.AddProperty(property)?;
        }
        cache.AddPattern(UIA_TextPatternId)?;
        let references = host.FindAllBuildCache(TreeScope_Descendants, &condition, &cache)?;
        let count = references.Length()?;
        if count > MAX_FOCUSED_REFERENCES {
            return Err(refusal("focused editor reference budget exceeded"));
        }
        tracing::debug!(
            foreground_process = process,
            focused_references = count,
            elapsed_ms = resolution_started.elapsed().as_millis() as u64,
            "resolving opaque focused host"
        );
        let mut candidates = Vec::new();
        let mut owners = std::collections::HashMap::new();
        for index in 0..count {
            let element = references.GetElement(index)?;
            if !element.CachedHasKeyboardFocus()?.as_bool() {
                return Err(refusal("descendant focus changed during resolution"));
            }
            if element.CachedIsPassword()?.as_bool() {
                return Err(refusal("focused descendant is protected"));
            }
            let editor_process = element.CachedProcessId()? as u32;
            let owner_matches = *owners
                .entry(editor_process)
                .or_insert_with(|| super::provider_owner_matches(process, editor_process));
            if !owner_matches {
                tracing::debug!(
                    foreground_process = process,
                    editor_process,
                    control_type = element.CachedControlType().ok().map(|value| value.0),
                    native_window = element
                        .CachedNativeWindowHandle()
                        .ok()
                        .map(|value| !value.0.is_null()),
                    text_pattern = element
                        .GetCachedPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                        .is_ok(),
                    "focused descendant owner mismatch"
                );
                return Err(refusal("focused descendant has a different owner"));
            }
            if element
                .GetCachedPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .is_ok()
            {
                if !candidate_safe(
                    [
                        element.CachedIsPassword().ok().map(|value| value.as_bool()),
                        element
                            .CachedIsOffscreen()
                            .ok()
                            .map(|value| value.as_bool()),
                        element.CachedIsEnabled().ok().map(|value| value.as_bool()),
                        element
                            .CachedHasKeyboardFocus()
                            .ok()
                            .map(|value| value.as_bool()),
                    ],
                    owner_matches,
                ) {
                    return Err(refusal("focused editor safety is unproved"));
                }
                let identity = cached_identity(&element);
                candidates.push((element, identity));
            }
        }
        let editor = unique_editor(candidates, |left, right| {
            automation
                .CompareElements(left, right)
                .map(|equal| equal.as_bool())
        })?;
        if !candidate_safe(
            [
                editor.CurrentIsPassword().ok().map(|value| value.as_bool()),
                editor
                    .CurrentIsOffscreen()
                    .ok()
                    .map(|value| value.as_bool()),
                editor.CurrentIsEnabled().ok().map(|value| value.as_bool()),
                editor
                    .CurrentHasKeyboardFocus()
                    .ok()
                    .map(|value| value.as_bool()),
            ],
            super::provider_owner_matches(process, editor.CurrentProcessId()? as u32),
        ) {
            return Err(refusal("focused editor changed after bulk snapshot"));
        }
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
        tracing::debug!(
            elapsed_ms = resolution_started.elapsed().as_millis() as u64,
            "opaque focused editor resolved"
        );
        if RESOLUTION_DEPTH.with(|depth| depth.get() > 0)
            && super::super::input_listener::current_position_generation() == key.position
            && super::super::input_listener::current_input_sequence() == key.sequence
        {
            if let Some(identity) = super::runtime_id(&editor) {
                RESOLVED_EDITOR.with(|cached| {
                    *cached.borrow_mut() = Some(ResolvedEditor {
                        key,
                        host,
                        editor: editor.clone(),
                        identity,
                    })
                });
            }
        }
        Ok(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_native_owner_or_focus_change_invalidates_discovery() {
        let key = ResolutionKey {
            window: 1,
            keyboard: 2,
            process: 3,
            position: 4,
            sequence: 5,
        };
        for changed in [
            ResolutionKey { window: 9, ..key },
            ResolutionKey { keyboard: 9, ..key },
            ResolutionKey { process: 9, ..key },
            ResolutionKey { position: 9, ..key },
            ResolutionKey { sequence: 9, ..key },
        ] {
            assert!(key != changed);
        }
        assert_eq!(RESOLUTION_DEPTH.with(|depth| depth.get()), 0);
        {
            let _scope = ResolutionScope::begin();
            assert_eq!(RESOLUTION_DEPTH.with(|depth| depth.get()), 1);
        }
        assert_eq!(RESOLUTION_DEPTH.with(|depth| depth.get()), 0);
        assert!(RESOLVED_EDITOR.with(|editor| editor.borrow().is_none()));
    }

    #[test]
    fn cached_identity_requires_a_nonempty_integer_array() {
        use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
        for invalid in [
            VARIANT::default(),
            VARIANT::from(42),
            VARIANT::from(true),
            VARIANT::from("42.7"),
        ] {
            assert_eq!(identity_from_variant(&invalid), None);
        }
        for values in [vec![], vec![42, 7, -3]] {
            unsafe {
                let mut value = VARIANT::default();
                let array = SafeArrayCreateVector(VT_I4, 0, values.len() as u32);
                assert!(!array.is_null());
                let raw = &mut *value.Anonymous.Anonymous;
                raw.vt = VT_ARRAY | VT_I4;
                raw.Anonymous.parray = array;
                for (index, item) in values.iter().enumerate() {
                    SafeArrayPutElement(array, &(index as i32), item as *const _ as *const _)
                        .unwrap();
                }
                assert_eq!(
                    identity_from_variant(&value),
                    if values.is_empty() {
                        None
                    } else {
                        Some("42.7.-3".into())
                    }
                );
            }
        }
    }

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
