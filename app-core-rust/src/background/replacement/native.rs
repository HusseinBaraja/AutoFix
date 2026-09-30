use super::{Attempt, ReplacementMethod, ReplacementPlan, ReplacementResult, ReplacementStrategy};

pub(super) struct NativeStrategy(pub(super) ReplacementMethod);

impl ReplacementStrategy for NativeStrategy {
    fn method(&self) -> ReplacementMethod {
        self.0
    }
    fn replace(&mut self, plan: &ReplacementPlan<'_>) -> Attempt {
        replace(self.0, plan)
    }
}

#[cfg(windows)]
fn replace(method: ReplacementMethod, plan: &ReplacementPlan<'_>) -> Attempt {
    use windows::Win32::{
        Foundation::{S_FALSE, S_OK},
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    };
    let initialization = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if initialization != S_OK && initialization != S_FALSE {
        return Attempt::Unavailable("UI Automation apartment unavailable".into());
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _apartment = Apartment;
    match PreparedRange::new(plan) {
        Err(reason) => Attempt::Unavailable(reason),
        Ok(range) => range.replace(method, plan),
    }
}

#[cfg(not(windows))]
fn replace(_: ReplacementMethod, _: &ReplacementPlan<'_>) -> Attempt {
    Attempt::Unavailable("native replacement requires Windows".into())
}

#[cfg(windows)]
use windows::Win32::UI::Accessibility::{
    IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationTextRange,
    TextPatternRangeEndpoint_End as END, TextPatternRangeEndpoint_Start as START,
    TextUnit_Character, UIA_TextPatternId,
};

#[cfg(windows)]
struct PreparedRange {
    automation: IUIAutomation2,
    element: IUIAutomationElement,
    pattern: IUIAutomationTextPattern,
    original_caret: IUIAutomationTextRange,
    span: IUIAutomationTextRange,
}

#[cfg(windows)]
impl PreparedRange {
    fn new(plan: &ReplacementPlan<'_>) -> Result<Self, String> {
        if plan.original.contains('\0') || plan.replacement.contains('\0') {
            return Err("embedded NUL cannot be replaced safely".into());
        }
        if !current(plan) {
            return Err("target or input changed before replacement".into());
        }
        let result = unsafe {
            (|| -> windows::core::Result<Self> {
                let automation = super::super::target::create_automation()?;
                let element = automation.GetFocusedElement()?;
                if element.CurrentIsPassword()?.as_bool()
                    || element.CurrentIsOffscreen()?.as_bool()
                    || !element.CurrentIsEnabled()?.as_bool()
                {
                    return Err(range_error("field is protected, hidden or disabled"));
                }
                let pattern: IUIAutomationTextPattern =
                    element.GetCurrentPatternAs(UIA_TextPatternId)?;
                let caret = collapsed_selection(&pattern)?;
                let span = caret.Clone()?;
                span.MoveEndpointByUnit(
                    START,
                    TextUnit_Character,
                    -(plan.range().start_back as i32),
                )?;
                span.MoveEndpointByUnit(END, TextUnit_Character, -(plan.range().end_back as i32))?;
                if span.GetText(-1)?.to_string() != plan.original {
                    return Err(range_error("pre-caret text does not match"));
                }
                // The frozen segment may precede newer typing, never guess its distance.
                let following = span.Clone()?;
                following.MoveEndpointByRange(START, &span, END)?;
                following.MoveEndpointByRange(END, &caret, END)?;
                if following.GetText(-1)?.to_string() != plan.following {
                    return Err(range_error("known following text does not match"));
                }
                Ok(Self {
                    automation,
                    element,
                    pattern,
                    original_caret: caret,
                    span,
                })
            })()
        };
        let range =
            result.map_err(|error| format!("no reliable collapsed pre-caret range: {error}"))?;
        if !current(plan) || !range.focused() {
            return Err("focus or input changed while proving range".into());
        }
        Ok(range)
    }

    fn focused(&self) -> bool {
        unsafe {
            self.automation
                .GetFocusedElement()
                .ok()
                .is_some_and(|element| {
                    self.automation
                        .CompareElements(&self.element, &element)
                        .is_ok_and(|equal| equal.as_bool())
                })
        }
    }

    fn replace(&self, method: ReplacementMethod, plan: &ReplacementPlan<'_>) -> Attempt {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        };
        // Never release the user's modifiers or inject text through a held hotkey.
        let released = std::time::Instant::now() + std::time::Duration::from_millis(250);
        while [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(*key)) } < 0)
        {
            if std::time::Instant::now() >= released || !current(plan) {
                return Attempt::Unavailable(
                    "shortcut modifiers remain held or input changed".into(),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let paste_window = if method == ReplacementMethod::Clipboard {
            match unsafe { self.element.CurrentNativeWindowHandle() } {
                Ok(window) if super::clipboard::supports_paste(window.0 as isize) => {
                    Some(window.0 as isize)
                }
                _ => {
                    return Attempt::Unavailable(
                        "no reliable synchronous clipboard paste API".into(),
                    )
                }
            }
        } else {
            if plan.replacement.contains(['\r', '\n', '\t']) {
                return Attempt::Unavailable("SendInput refuses control characters".into());
            }
            None
        };
        // Snapshot every clipboard format before selecting anything in the target.
        let mut clipboard = if paste_window.is_some() {
            match super::clipboard::ClipboardTransaction::begin(plan.replacement) {
                Ok(transaction) => Some(transaction),
                Err(failure) if !failure.clipboard_uncertain => {
                    return Attempt::Unavailable(failure.reason)
                }
                Err(failure) => {
                    return Attempt::Finished(ReplacementResult {
                        success: false,
                        method: Some(method),
                        range: Some(plan.range()),
                        reason: Some(failure.reason),
                        may_have_changed: true,
                    })
                }
            }
        } else {
            None
        };
        let mut result = ReplacementResult {
            success: false,
            method: Some(method),
            range: Some(plan.range()),
            reason: None,
            may_have_changed: false,
        };
        let operation = (|| -> Result<(), String> {
            if !current(plan) || !self.focused() {
                return Err("target changed before selection".into());
            }
            unsafe { self.span.Select() }.map_err(|_| "range selection failed")?;
            // Verify Select selected exactly the proven span, not a provider approximation.
            let selected =
                unsafe { self.pattern.GetSelection() }.map_err(|_| "selection unreadable")?;
            if unsafe { selected.Length() }.ok() != Some(1) {
                return Err("selection is not a single range".into());
            }
            let selected = unsafe { selected.GetElement(0) }.map_err(|_| "selection unreadable")?;
            if unsafe { selected.CompareEndpoints(START, &self.span, START) }.ok() != Some(0)
                || unsafe { selected.CompareEndpoints(END, &self.span, END) }.ok() != Some(0)
                || unsafe { selected.GetText(-1) }
                    .map_err(|_| "selection unreadable")?
                    .to_string()
                    != plan.original
                || !current(plan)
                || !self.focused()
            {
                return Err("selected range or input changed".into());
            }
            // After this point a failed call may have mutated text: never fall through.
            result.may_have_changed = true;
            if let Some(window) = paste_window {
                super::clipboard::paste(window)?;
            } else {
                send_text(plan.replacement)?;
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            loop {
                if !current(plan) || !self.focused() {
                    return Err("target or input changed during replacement".into());
                }
                let failure = match self.verify_and_restore_caret(plan) {
                    Ok(()) => return Ok(()),
                    Err(reason) => reason,
                };
                if std::time::Instant::now() >= deadline {
                    return Err(format!("replacement could not be verified: {failure}"));
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        })();
        if operation.is_err() && !result.may_have_changed && current(plan) && self.focused() {
            if unsafe { self.original_caret.Select() }.is_err() {
                result.may_have_changed = true;
            }
        }
        let restored = clipboard
            .as_mut()
            .map_or(Ok(()), |transaction| transaction.restore());
        match (operation, restored) {
            (Ok(()), Ok(())) => result.success = true,
            (Err(reason), Ok(())) | (Ok(()), Err(reason)) => result.reason = Some(reason),
            (Err(reason), Err(restore)) => result.reason = Some(format!("{reason}; {restore}")),
        }
        Attempt::Finished(result)
    }

    fn verify_and_restore_caret(&self, plan: &ReplacementPlan<'_>) -> Result<(), String> {
        unsafe {
            (|| -> windows::core::Result<()> {
                // Discard the pre-mutation provider and its range snapshots.
                let automation = super::super::target::create_automation()?;
                let element = automation.GetFocusedElement()?;
                let pattern: IUIAutomationTextPattern =
                    element.GetCurrentPatternAs(UIA_TextPatternId)?;
                let caret = collapsed_selection(&pattern)?;
                let preceding = caret.Clone()?;
                preceding.MoveEndpointByUnit(
                    START,
                    TextUnit_Character,
                    -(plan.replacement.chars().count() as i32),
                )?;
                if preceding.GetText(-1)?.to_string() != plan.replacement {
                    return Err(range_error("inserted prefix does not match"));
                }
                let following = caret.Clone()?;
                following.MoveEndpointByUnit(
                    END,
                    TextUnit_Character,
                    plan.following.chars().count() as i32,
                )?;
                if following.GetText(-1)?.to_string() != plan.following {
                    return Err(range_error("following text does not match"));
                }
                if !current(plan) || !self.focused() {
                    return Err(range_error("target changed before caret restoration"));
                }
                following.MoveEndpointByRange(START, &following, END)?;
                following.Select()?;
                let restored = collapsed_selection(&pattern)?;
                if restored.CompareEndpoints(END, &following, END)? != 0
                    || !current(plan)
                    || !self.focused()
                {
                    return Err(range_error("caret restoration could not be verified"));
                }
                Ok(())
            })()
            .map_err(|error| error.to_string())
        }
    }
}

#[cfg(windows)]
fn range_error(message: &str) -> windows::core::Error {
    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), message)
}

#[cfg(windows)]
unsafe fn collapsed_selection(
    pattern: &IUIAutomationTextPattern,
) -> windows::core::Result<IUIAutomationTextRange> {
    let selections = pattern.GetSelection()?;
    if selections.Length()? != 1 {
        return Err(range_error("selection is not a single caret"));
    }
    let caret = selections.GetElement(0)?;
    if caret.CompareEndpoints(START, &caret, END)? != 0 {
        return Err(range_error("selection is not collapsed"));
    }
    Ok(caret)
}

#[cfg(windows)]
fn current(plan: &ReplacementPlan<'_>) -> bool {
    use super::super::{
        input_listener,
        target::{detect_focused_target, CorrectionEligibility, TargetDetection},
    };
    input_listener::current_position_generation() == plan.stamp.position
        && input_listener::current_input_sequence() == plan.stamp.sequence
        && matches!(detect_focused_target(), TargetDetection::Available(target)
            if target.correction_eligibility() == CorrectionEligibility::Allowed
                && target.process_id == plan.target.process_id
                && target.window_handle == plan.target.window_handle
                && target.focused_element_id == plan.target.focused_element_id)
}

#[cfg(windows)]
fn send_text(text: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VK_BACK,
    };
    let mut inputs = Vec::new();
    let units: Vec<_> = text.encode_utf16().collect();
    for unit in if units.is_empty() { vec![0] } else { units } {
        for up in [false, true] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: if text.is_empty() { VK_BACK } else { 0 },
                        wScan: unit,
                        dwFlags: if text.is_empty() {
                            0
                        } else {
                            KEYEVENTF_UNICODE
                        } | if up { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(format!(
            "SendInput inserted {sent} of {} events (possibly blocked by UIPI)",
            inputs.len()
        ))
    }
}
