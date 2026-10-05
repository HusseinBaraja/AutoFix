use super::{Attempt, ReplacementMethod, ReplacementPlan, ReplacementResult, ReplacementStrategy};

pub(super) struct NativeStrategy(pub(super) ReplacementMethod);

impl ReplacementStrategy for NativeStrategy {
    /// Identify the mutation method for strategy ordering and failure metadata.
    fn method(&self) -> ReplacementMethod {
        self.0
    }

    /// Attempt only the requested native method after proving the pre-caret range.
    fn replace(&mut self, plan: &ReplacementPlan<'_>) -> Attempt {
        replace(self.0, plan)
    }
}

/// Attempt only the requested native method after proving the pre-caret range.
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
        /// Balance the COM initialization on the thread that acquired it.
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

/// Refuse native mutation on platforms without Windows replacement APIs.
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
    original_selection: IUIAutomationTextRange,
    span: IUIAutomationTextRange,
}

#[cfg(windows)]
impl PreparedRange {
    /// Prove exact session text before a collapsed caret or a verified selection end.
    fn new(plan: &ReplacementPlan<'_>) -> Result<Self, String> {
        if plan.original.contains('\0') || plan.replacement.contains('\0') {
            return Err("embedded NUL cannot be replaced safely".into());
        }
        if plan.original.is_empty() && plan.replacement.is_empty() {
            return Err("empty span cannot authorize a deletion".into());
        }
        if plan.range().start_back > 65_536 || plan.replacement.encode_utf16().count() > 65_536 {
            return Err("replacement range exceeds the input budget".into());
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
                    || element.CurrentProcessId()? as u32 != plan.target.process_id
                {
                    return Err(range_error("field is protected, hidden or disabled"));
                }
                let pattern: IUIAutomationTextPattern =
                    element.GetCurrentPatternAs(UIA_TextPatternId)?;
                let selections = pattern.GetSelection()?;
                if selections.Length()? != 1 {
                    return Err(range_error("selection is not a single range"));
                }
                let original_selection = selections.GetElement(0)?;
                let caret = if plan.selected_text {
                    let selection = &original_selection;
                    if selection.GetText(-1)? != plan.original || !plan.following.is_empty() {
                        return Err(range_error("selection does not match executable text"));
                    }
                    super::super::context_capture::selected_caret(&element, selection)?
                } else {
                    collapsed_selection(&pattern)?
                };
                // Providers differ on supplementary Unicode character units.
                // Resolve both known spans independently and require exact text.
                let following = adjacent_range(&caret, plan.following, true)?;
                let original_end = following.Clone()?;
                original_end.MoveEndpointByRange(END, &following, START)?;
                let span = adjacent_range(&original_end, plan.original, true)?;
                if span.CompareEndpoints(START, &span, END)? > 0
                    || span.CompareEndpoints(END, &caret, END)? > 0
                    || span.GetText(-1)? != plan.original
                    || (plan.selected_text
                        && (span.CompareEndpoints(START, &original_selection, START)? != 0
                            || span.CompareEndpoints(END, &original_selection, END)? != 0))
                {
                    return Err(range_error("pre-caret text does not match"));
                }
                Ok(Self {
                    automation,
                    element,
                    pattern,
                    original_caret: caret,
                    original_selection,
                    span,
                })
            })()
        };
        // Provider error messages are untrusted and can contain document text.
        let range = result.map_err(|_| "no reliable pre-caret range".to_owned())?;
        super::ui_automation::writable(&range.span)?;
        if !current(plan) || !range.focused() {
            return Err("focus or input changed while proving range".into());
        }
        Ok(range)
    }

    /// Require the same focused UI Automation element throughout the operation.
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

    /// Attempt only the requested native method after proving the pre-caret range.
    fn replace(&self, method: ReplacementMethod, plan: &ReplacementPlan<'_>) -> Attempt {
        // Never release the user's modifiers or inject text through a held hotkey.
        let released = std::time::Instant::now() + std::time::Duration::from_millis(250);
        while super::send_input::modifiers_held() {
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
            None
        };
        let direct_window = if method == ReplacementMethod::DirectTextApi {
            match unsafe { self.element.CurrentNativeWindowHandle() } {
                Ok(window) if super::direct::supports_target(window.0 as isize) => {
                    Some(window.0 as isize)
                }
                _ => return Attempt::Unavailable("no safe selected-range native text API".into()),
            }
        } else {
            None
        };
        let direct_text = if let Some(window) = direct_window {
            match super::direct::prepare(window, plan.replacement) {
                Ok(text) => Some(text),
                Err(reason) => return Attempt::Unavailable(reason),
            }
        } else {
            None
        };
        if method == ReplacementMethod::UiAutomation {
            if let Err(reason) = super::ui_automation::authorize(
                &self.element,
                &self.pattern,
                &self.span,
                plan.following,
            ) {
                return Attempt::Unavailable(reason);
            }
            if !provider_keyboard_target(plan) {
                return Attempt::Unavailable(
                    "UIA keyboard host is not the authorized process".into(),
                );
            }
        }
        let input_window = if method == ReplacementMethod::SendInput {
            match unsafe { self.element.CurrentNativeWindowHandle() } {
                Ok(window) if super::send_input::supports_target(window.0 as isize) => {
                    Some(window.0 as isize)
                }
                _ => {
                    return Attempt::Unavailable(
                        "SendInput target has no verified insert semantics".into(),
                    )
                }
            }
        } else {
            None
        };
        let inputs = if input_window.is_some() || method == ReplacementMethod::UiAutomation {
            match super::send_input::prepare(plan.replacement) {
                Ok(inputs) => Some(inputs),
                Err(reason) => return Attempt::Unavailable(reason),
            }
        } else {
            None
        };
        // Snapshot every clipboard format before selecting anything in the target.
        let mut clipboard = if paste_window.is_some() {
            match super::clipboard::ClipboardTransaction::prepare(plan.replacement) {
                Ok(transaction) => Some(transaction),
                Err(reason) => return Attempt::Unavailable(reason),
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
        let mut selection_attempted = false;
        let operation = (|| -> Result<(), String> {
            if !current(plan) || !self.focused() {
                return Err("target changed before selection".into());
            }
            // Preparation must not silently replace a newer programmatic selection.
            self.verify_original_selection(plan)?;
            selection_attempted = true;
            unsafe { self.span.Select() }.map_err(|_| "range selection failed")?;
            // Verify Select selected exactly the proven span, not a provider approximation.
            self.verify_selection(plan)?;
            if let Some(transaction) = clipboard.as_mut() {
                transaction.install().map_err(|failure| {
                    if failure.clipboard_uncertain {
                        super::clipboard::restore_failed(
                            &plan.target.process_name,
                            plan.target.process_id,
                        );
                    }
                    failure.reason
                })?;
            }
            // Clipboard preparation and provider calls can take time. Recheck the
            // exact selection and user modifiers at the last mutation boundary.
            self.verify_selection(plan)?;
            if super::send_input::modifiers_held() {
                return Err("shortcut modifier pressed before mutation".into());
            }
            if let Some(window) = input_window {
                if !super::send_input::supports_target(window) || !keyboard_target(window, plan) {
                    return Err("SendInput keyboard focus or target safety changed".into());
                }
            }
            if let Some(window) = direct_window {
                if !super::direct::supports_target(window) || !keyboard_target(window, plan) {
                    return Err("native insertion focus or target safety changed".into());
                }
                super::direct::validate_selection(window, plan.original, plan.replacement)?;
            }
            if method == ReplacementMethod::UiAutomation {
                super::ui_automation::authorize(
                    &self.element,
                    &self.pattern,
                    &self.span,
                    plan.following,
                )?;
                if !provider_keyboard_target(plan) {
                    return Err("UIA keyboard host changed before mutation".into());
                }
            }
            super::ui_automation::writable(&self.span)?;
            if !current(plan) || !self.focused() {
                return Err("target or input changed before mutation".into());
            }
            self.verify_selection(plan)?;
            // After this point a failed call may have mutated text: never fall through.
            result.may_have_changed = true;
            if let Some(window) = direct_window {
                super::direct::insert(
                    window,
                    direct_text
                        .as_deref()
                        .ok_or("native text batch unavailable")?,
                )?;
            } else if let Some(window) = paste_window {
                super::clipboard::paste_and_restore(
                    || super::clipboard::paste(window),
                    || restore_clipboard(clipboard.as_mut(), plan),
                )?;
            } else {
                super::send_input::send(inputs.as_deref().ok_or("SendInput batch unavailable")?)?;
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            loop {
                if !current_after_mutation(plan) || !self.focused() {
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
        let restored = restore_clipboard(clipboard.as_mut(), plan);
        if operation.is_err() && !result.may_have_changed && selection_attempted {
            // An unverified selection is enough to invalidate the typed anchor.
            result.may_have_changed = !self.restore_original_caret(plan);
        }
        match (operation, restored) {
            (Ok(()), Ok(())) => result.success = true,
            (Err(reason), Ok(())) | (Ok(()), Err(reason)) => result.reason = Some(reason),
            (Err(reason), Err(restore)) => result.reason = Some(format!("{reason}; {restore}")),
        }
        Attempt::Finished(result)
    }

    /// Undo selection only while the original target and input stamp remain current.
    fn restore_original_caret(&self, plan: &ReplacementPlan<'_>) -> bool {
        if !current(plan) || !self.focused() {
            return false;
        }
        unsafe {
            self.original_selection.Select().is_ok()
                && self.verify_original_selection(plan).is_ok()
                && current(plan)
                && self.focused()
        }
    }

    /// Preserve the original selection and its active endpoint until mutation begins.
    fn verify_original_selection(&self, plan: &ReplacementPlan<'_>) -> Result<(), String> {
        unsafe {
            let selections = self
                .pattern
                .GetSelection()
                .map_err(|_| "selection unreadable")?;
            if selections.Length().ok() != Some(1) {
                return Err("selection changed during preparation".into());
            }
            let selection = selections
                .GetElement(0)
                .map_err(|_| "selection unreadable")?;
            if selection
                .CompareEndpoints(START, &self.original_selection, START)
                .ok()
                != Some(0)
                || selection
                    .CompareEndpoints(END, &self.original_selection, END)
                    .ok()
                    != Some(0)
                || (plan.selected_text
                    && selection
                        .GetText(-1)
                        .ok()
                        .as_ref()
                        .is_none_or(|text| *text != plan.original))
            {
                return Err("selection changed during preparation".into());
            }
            let caret = if plan.selected_text {
                super::super::context_capture::selected_caret(&self.element, &selection)
            } else {
                collapsed_selection(&self.pattern)
            }
            .map_err(|_| "caret endpoint unavailable")?;
            if caret.CompareEndpoints(END, &self.original_caret, END).ok() != Some(0) {
                return Err("caret endpoint changed during preparation".into());
            }
        }
        Ok(())
    }

    /// Reject approximate provider selections before publishing or injecting text.
    fn verify_selection(&self, plan: &ReplacementPlan<'_>) -> Result<(), String> {
        unsafe {
            let selections = self
                .pattern
                .GetSelection()
                .map_err(|_| "selection unreadable")?;
            if selections.Length().ok() != Some(1) {
                return Err("selection is not a single range".into());
            }
            let selected = selections
                .GetElement(0)
                .map_err(|_| "selection unreadable")?;
            if selected.CompareEndpoints(START, &self.span, START).ok() != Some(0)
                || selected.CompareEndpoints(END, &self.span, END).ok() != Some(0)
                || selected.GetText(-1).map_err(|_| "selection unreadable")? != plan.original
                || !current(plan)
                || !self.focused()
            {
                return Err("selected range or input changed".into());
            }
            if plan.selected_text {
                super::super::context_capture::selected_caret(&self.element, &selected)
                    .map_err(|_| "selected caret endpoint changed")?;
            }
        }
        Ok(())
    }

    /// Reacquire the provider and prove replacement and following text before moving the caret.
    fn verify_and_restore_caret(&self, plan: &ReplacementPlan<'_>) -> Result<(), String> {
        unsafe {
            (|| -> windows::core::Result<()> {
                // Discard the pre-mutation provider and its range snapshots.
                let automation = super::super::target::create_automation()?;
                let element = automation.GetFocusedElement()?;
                let pattern: IUIAutomationTextPattern =
                    element.GetCurrentPatternAs(UIA_TextPatternId)?;
                let caret = collapsed_selection(&pattern)?;
                adjacent_range(&caret, plan.replacement, true)?;
                let following = adjacent_range(&caret, plan.following, false)?;
                if !current_after_mutation(plan) || !self.focused() {
                    return Err(range_error("target changed before caret restoration"));
                }
                following.MoveEndpointByRange(START, &following, END)?;
                following.Select()?;
                let restored = collapsed_selection(&pattern)?;
                if restored.CompareEndpoints(END, &following, END)? != 0
                    || !current_after_mutation(plan)
                    || !self.focused()
                {
                    return Err(range_error("caret restoration could not be verified"));
                }
                Ok(())
            })()
            .map_err(|_| "native replacement or caret verification failed".to_owned())
        }
    }
}

/// Restore saved formats and disable this app's clipboard strategy after failure.
#[cfg(windows)]
fn restore_clipboard(
    clipboard: Option<&mut super::clipboard::ClipboardTransaction>,
    plan: &ReplacementPlan<'_>,
) -> Result<(), String> {
    let result = clipboard.map_or(Ok(()), |transaction| transaction.restore());
    if result.is_err() {
        super::clipboard::restore_failed(&plan.target.process_name, plan.target.process_id);
    }
    result
}

/// Create internal range errors; discard provider error text at the public boundary.
#[cfg(windows)]
fn range_error(message: &str) -> windows::core::Error {
    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), message)
}

/// Resolve a known adjacent span, never search document text or move the caret.
/// Accept only exact text at the fixed anchor, using scalar or UTF-16 units.
#[cfg(windows)]
unsafe fn adjacent_range(
    anchor: &IUIAutomationTextRange,
    text: &str,
    before: bool,
) -> windows::core::Result<IUIAutomationTextRange> {
    if anchor.CompareEndpoints(START, anchor, END)? != 0 {
        return Err(range_error("range anchor is not collapsed"));
    }
    for count in [text.chars().count(), text.encode_utf16().count()] {
        let count = i32::try_from(count).map_err(|_| range_error("range exceeds input budget"))?;
        let range = anchor.Clone()?;
        range.MoveEndpointByUnit(
            if before { START } else { END },
            TextUnit_Character,
            if before { -count } else { count },
        )?;
        if range.GetText(-1)? == text {
            return Ok(range);
        }
    }
    Err(range_error("adjacent range does not match known text"))
}

/// Accept exactly one empty selection, with no assumption about its active endpoint.
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

/// Recheck input generations and the complete authorized target before native actions.
#[cfg(windows)]
fn current(plan: &ReplacementPlan<'_>) -> bool {
    current_target(plan, same_authorized_target)
}

/// Verify the same eligible field after edits may have marked its title modified.
#[cfg(windows)]
fn current_after_mutation(plan: &ReplacementPlan<'_>) -> bool {
    current_target(plan, same_mutated_target)
}

/// Bracket target detection with input and shutdown checks for either operation phase.
#[cfg(windows)]
fn current_target(
    plan: &ReplacementPlan<'_>,
    matches_target: impl FnOnce(&super::FocusedTarget, &super::FocusedTarget) -> bool,
) -> bool {
    use super::super::{
        input_listener,
        target::{detect_focused_target, TargetDetection},
    };
    !super::shutting_down()
        && input_listener::current_position_generation() == plan.stamp.position
        && input_listener::current_input_sequence() == plan.stamp.sequence
        && matches!(detect_focused_target(), TargetDetection::Available(target)
            if matches_target(plan.target, &target))
        && input_listener::current_position_generation() == plan.stamp.position
        && input_listener::current_input_sequence() == plan.stamp.sequence
        && !super::shutting_down()
}

/// Bind mutation to every policy-bearing attribute of the authorized target.
#[cfg(any(windows, test))]
pub(super) fn same_authorized_target(
    expected: &super::FocusedTarget,
    current: &super::FocusedTarget,
) -> bool {
    current.correction_eligibility() == super::CorrectionEligibility::Allowed && current == expected
}

/// After mutation only the title may change; identity and every safety flag remain bound.
#[cfg(any(windows, test))]
pub(super) fn same_mutated_target(
    expected: &super::FocusedTarget,
    current: &super::FocusedTarget,
) -> bool {
    let mut retitled = current.clone();
    retitled.window_title.clone_from(&expected.window_title);
    same_authorized_target(expected, &retitled)
}

/// Confirm native keyboard focus belongs to the authorized process and control.
#[cfg(windows)]
fn keyboard_target(window: isize, plan: &ReplacementPlan<'_>) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    };
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground as isize != plan.target.window_handle {
            return false;
        }
        let mut process = 0;
        let thread = GetWindowThreadProcessId(foreground, &mut process);
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        process == plan.target.process_id
            && thread != 0
            && GetGUIThreadInfo(thread, &mut info) != 0
            && info.hwndFocus as isize == window
    }
}

/// Windowless UIA controls still need a live foreground keyboard host in the same process.
#[cfg(windows)]
fn provider_keyboard_target(plan: &ReplacementPlan<'_>) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    };
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground as isize != plan.target.window_handle {
            return false;
        }
        let mut process = 0;
        let thread = GetWindowThreadProcessId(foreground, &mut process);
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if process != plan.target.process_id
            || thread == 0
            || GetGUIThreadInfo(thread, &mut info) == 0
            || info.hwndFocus.is_null()
        {
            return false;
        }
        let mut focused_process = 0;
        GetWindowThreadProcessId(info.hwndFocus, &mut focused_process);
        focused_process == plan.target.process_id
    }
}
