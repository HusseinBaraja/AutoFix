use std::path::Path;

use windows::Win32::{
    Foundation::{S_FALSE, S_OK},
    System::{
        Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED, SAFEARRAY,
        },
        Ole::{SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound},
    },
    UI::Accessibility::{CUIAutomation, IUIAutomation},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, HWND},
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    System::{
        StationsAndDesktops::{
            CloseDesktop, GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop,
            DESKTOP_SWITCHDESKTOP, UOI_NAME,
        },
        Threading::{
            GetCurrentThreadId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
    UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusedTarget {
    pub(crate) process_id: u32,
    pub(crate) process_name: String,
    pub(crate) window_handle: isize,
    pub(crate) window_title: String,
    pub(crate) focused_element_id: Option<FocusedElementId>,
    pub(crate) is_elevated: bool,
    pub(crate) is_password_or_protected: bool,
    pub(crate) is_hidden_or_unavailable: bool,
    pub(crate) field_safety_known: bool,
    pub(crate) is_secure_desktop: bool,
    pub(crate) is_lock_screen: bool,
    pub(crate) is_credential_dialog: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusedElementId {
    RuntimeId(String),
    AutomationId(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionKey {
    FocusedElement(String),
    WindowHandle(isize),
    ProcessTitle {
        process_id: u32,
        window_title: String,
    },
    TemporaryActiveSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CorrectionEligibility {
    Allowed,
    BlockedElevated,
    BlockedProtectedField,
    BlockedHiddenOrUnavailable,
    BlockedSecureDesktop,
    BlockedLockScreen,
    BlockedCredentialDialog,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetDetection {
    Available(FocusedTarget),
    Unsupported,
}

impl FocusedTarget {
    pub(crate) fn session_key(&self) -> SessionKey {
        session_key_for(
            self.focused_element_id.as_ref(),
            Some(self.window_handle),
            Some(self.process_id),
            Some(&self.window_title),
        )
    }

    pub(crate) fn correction_eligibility(&self) -> CorrectionEligibility {
        if self.is_secure_desktop {
            CorrectionEligibility::BlockedSecureDesktop
        } else if self.is_lock_screen {
            CorrectionEligibility::BlockedLockScreen
        } else if self.is_credential_dialog {
            CorrectionEligibility::BlockedCredentialDialog
        } else if self.is_elevated {
            CorrectionEligibility::BlockedElevated
        } else if self.is_password_or_protected {
            CorrectionEligibility::BlockedProtectedField
        } else if self.is_hidden_or_unavailable {
            CorrectionEligibility::BlockedHiddenOrUnavailable
        } else if !self.field_safety_known {
            CorrectionEligibility::Unsupported
        } else {
            CorrectionEligibility::Allowed
        }
    }
}

pub(crate) fn detect_focused_target() -> TargetDetection {
    let Some(window) = active_window_handle() else {
        return TargetDetection::Unsupported;
    };

    let Some(process_id) = window_process_id(window) else {
        return TargetDetection::Unsupported;
    };

    let process_name = process_name(process_id).unwrap_or_else(|| "Unknown app".to_owned());
    let window_title = window_title(window).unwrap_or_default();
    let is_elevated = process_is_elevated_or_blocked(process_id);
    let desktop_state = desktop_state();
    let element = focused_element_context();
    let (
        focused_element_id,
        is_password_or_protected,
        is_hidden_or_unavailable,
        field_safety_known,
    ) = element
        .map(|element| {
            (
                element.focused_element_id,
                element.is_password_or_protected,
                element.is_hidden_or_unavailable,
                true,
            )
        })
        .unwrap_or((None, false, false, false));
    let normalized_process = normalize_process_name(&process_name);
    let normalized_title = window_title.to_ascii_lowercase();

    TargetDetection::Available(FocusedTarget {
        process_id,
        process_name,
        window_handle: window as isize,
        window_title,
        focused_element_id,
        is_elevated,
        is_password_or_protected,
        is_hidden_or_unavailable,
        field_safety_known,
        is_secure_desktop: desktop_state.is_secure,
        is_lock_screen: is_lock_screen_process(&normalized_process),
        is_credential_dialog: is_credential_context(&normalized_process, &normalized_title),
    })
}

pub(crate) fn session_key_for(
    focused_element_id: Option<&FocusedElementId>,
    window_handle: Option<isize>,
    process_id: Option<u32>,
    window_title: Option<&str>,
) -> SessionKey {
    if let Some(focused_element_id) = focused_element_id {
        return SessionKey::FocusedElement(focused_element_id.as_session_value());
    }

    if let Some(window_handle) = window_handle.filter(|handle| *handle != 0) {
        return SessionKey::WindowHandle(window_handle);
    }

    if let (Some(process_id), Some(window_title)) = (process_id, window_title) {
        if !window_title.trim().is_empty() {
            return SessionKey::ProcessTitle {
                process_id,
                window_title: window_title.to_owned(),
            };
        }
    }

    SessionKey::TemporaryActiveSession
}

impl FocusedElementId {
    fn as_session_value(&self) -> String {
        match self {
            Self::RuntimeId(value) => format!("runtime:{value}"),
            Self::AutomationId(value) => format!("automation:{value}"),
        }
    }
}

struct FocusedElementContext {
    focused_element_id: Option<FocusedElementId>,
    is_password_or_protected: bool,
    is_hidden_or_unavailable: bool,
}

struct DesktopState {
    is_secure: bool,
}

fn active_window_handle() -> Option<HWND> {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            None
        } else {
            Some(window)
        }
    }
}

fn window_process_id(window: HWND) -> Option<u32> {
    unsafe {
        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        (process_id != 0).then_some(process_id)
    }
}

fn process_name(process_id: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return None;
        }

        let mut buffer = vec![0_u16; 260];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);

        if result == 0 || length == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
    }
}

fn window_title(window: HWND) -> Option<String> {
    unsafe {
        let length = GetWindowTextLengthW(window);
        if length <= 0 {
            return None;
        }

        let mut buffer = vec![0_u16; length as usize + 1];
        let copied = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32);
        if copied <= 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&buffer[..copied as usize]))
    }
}

fn process_is_elevated_or_blocked(process_id: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }

        let mut token = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            let access_denied = GetLastError() == ERROR_ACCESS_DENIED;
            CloseHandle(process);
            return access_denied;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned_size = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned_size,
        );
        let access_denied = ok == 0 && GetLastError() == ERROR_ACCESS_DENIED;
        CloseHandle(token);
        CloseHandle(process);

        access_denied || (ok != 0 && elevation.TokenIsElevated != 0)
    }
}

fn focused_element_context() -> Option<FocusedElementContext> {
    unsafe {
        let initialization_result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let initialization_succeeded =
            initialization_result == S_OK || initialization_result == S_FALSE;
        if !accept_com_initialization(initialization_result) {
            return None;
        }

        let context = (|| {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let element = automation.GetFocusedElement().ok()?;

            let focused_element_id = runtime_id(&element)
                .map(FocusedElementId::RuntimeId)
                .or_else(|| automation_id(&element).map(FocusedElementId::AutomationId));
            let is_password_or_protected = element
                .CurrentIsPassword()
                .map(|is_password| is_password.as_bool())
                .unwrap_or(false);
            let is_offscreen = element
                .CurrentIsOffscreen()
                .map(|is_offscreen| is_offscreen.as_bool())
                .unwrap_or(true);
            let is_enabled = element
                .CurrentIsEnabled()
                .map(|is_enabled| is_enabled.as_bool())
                .unwrap_or(false);

            Some(FocusedElementContext {
                focused_element_id,
                is_password_or_protected,
                is_hidden_or_unavailable: is_offscreen || !is_enabled,
            })
        })();

        if initialization_succeeded {
            CoUninitialize();
        }

        context
    }
}

fn desktop_state() -> DesktopState {
    unsafe {
        let input_desktop = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if input_desktop.is_null() {
            return DesktopState { is_secure: true };
        }

        let current_desktop = GetThreadDesktop(GetCurrentThreadId());
        let is_secure = match (
            desktop_name(current_desktop),
            desktop_name(input_desktop as *mut _),
        ) {
            (Some(current), Some(input)) => current != input,
            _ => true,
        };
        CloseDesktop(input_desktop);

        DesktopState { is_secure }
    }
}

fn desktop_name(desktop: *mut std::ffi::c_void) -> Option<String> {
    if desktop.is_null() {
        return None;
    }

    unsafe {
        let mut needed = 0;
        let _ = GetUserObjectInformationW(desktop, UOI_NAME, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            return None;
        }

        let mut buffer = vec![0_u16; needed as usize / std::mem::size_of::<u16>()];
        if GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            buffer.as_mut_ptr() as *mut _,
            needed,
            &mut needed,
        ) == 0
        {
            return None;
        }

        let length = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..length]))
    }
}

fn normalize_process_name(process_name: &str) -> String {
    process_name.trim().to_ascii_lowercase()
}

fn is_lock_screen_process(process_name: &str) -> bool {
    matches!(
        process_name,
        "logonui.exe" | "lockapp.exe" | "winlogon.exe" | "logonui"
    )
}

fn is_credential_context(process_name: &str, window_title: &str) -> bool {
    let process_name = normalize_process_name(process_name);
    let window_title = window_title.to_ascii_lowercase();
    matches!(
        process_name.as_str(),
        "credentialui.exe" | "credwiz.exe" | "consent.exe" | "lsass.exe"
    ) || window_title.contains("credential")
        || window_title.contains("credentials")
        || window_title.contains("windows security")
        || window_title.contains("user account control")
        || window_title.contains("uac")
}

fn accept_com_initialization(initialization_result: windows::core::HRESULT) -> bool {
    initialization_result == S_OK || initialization_result == S_FALSE
}

fn runtime_id(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Option<String> {
    unsafe {
        let runtime_id = element.GetRuntimeId().ok()?;
        let values = safe_array_i32_values(runtime_id);
        let _ = SafeArrayDestroy(runtime_id);

        let values = values?;
        (!values.is_empty()).then(|| {
            values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(".")
        })
    }
}

fn automation_id(
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> Option<String> {
    unsafe {
        let automation_id = element.CurrentAutomationId().ok()?.to_string();
        let trimmed = automation_id.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }
}

fn safe_array_i32_values(safe_array: *mut SAFEARRAY) -> Option<Vec<i32>> {
    if safe_array.is_null() {
        return None;
    }

    unsafe {
        if windows::Win32::System::Ole::SafeArrayGetDim(safe_array) != 1 {
            return None;
        }

        let lower_bound = SafeArrayGetLBound(safe_array, 1).ok()?;
        let upper_bound = SafeArrayGetUBound(safe_array, 1).ok()?;
        if upper_bound < lower_bound {
            return Some(Vec::new());
        }

        let mut values = Vec::with_capacity((upper_bound - lower_bound + 1) as usize);
        for index in lower_bound..=upper_bound {
            let mut value = 0_i32;
            SafeArrayGetElement(
                safe_array,
                &index,
                &mut value as *mut _ as *mut std::ffi::c_void,
            )
            .ok()?;
            values.push(value);
        }

        Some(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::HRESULT;

    fn normal_target() -> FocusedTarget {
        FocusedTarget {
            process_id: 42,
            process_name: "notepad.exe".to_owned(),
            window_handle: 123,
            window_title: "Notes".to_owned(),
            focused_element_id: None,
            is_elevated: false,
            is_password_or_protected: false,
            is_hidden_or_unavailable: false,
            field_safety_known: true,
            is_secure_desktop: false,
            is_lock_screen: false,
            is_credential_dialog: false,
        }
    }

    #[test]
    fn session_key_prefers_focused_element_id() {
        let element_id = FocusedElementId::RuntimeId("1.2.3".to_owned());

        assert_eq!(
            session_key_for(Some(&element_id), Some(123), Some(42), Some("Notes")),
            SessionKey::FocusedElement("runtime:1.2.3".to_owned())
        );
    }

    #[test]
    fn session_key_uses_window_handle_fallback() {
        assert_eq!(
            session_key_for(None, Some(123), Some(42), Some("Notes")),
            SessionKey::WindowHandle(123)
        );
    }

    #[test]
    fn session_key_uses_process_title_fallback() {
        assert_eq!(
            session_key_for(None, None, Some(42), Some("Notes")),
            SessionKey::ProcessTitle {
                process_id: 42,
                window_title: "Notes".to_owned()
            }
        );
    }

    #[test]
    fn session_key_uses_temporary_active_session_fallback() {
        assert_eq!(
            session_key_for(None, None, Some(42), Some(" ")),
            SessionKey::TemporaryActiveSession
        );
    }

    #[test]
    fn elevated_target_blocks_correction() {
        let mut target = normal_target();
        target.is_elevated = true;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::BlockedElevated
        );
    }

    #[test]
    fn password_or_protected_target_blocks_correction() {
        let mut target = normal_target();
        target.is_password_or_protected = true;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::BlockedProtectedField
        );
    }

    #[test]
    fn hidden_or_unavailable_target_blocks_correction() {
        let mut target = normal_target();
        target.is_hidden_or_unavailable = true;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::BlockedHiddenOrUnavailable
        );
    }

    #[test]
    fn unknown_field_safety_blocks_as_unsupported() {
        let mut target = normal_target();
        target.field_safety_known = false;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::Unsupported
        );
    }

    #[test]
    fn secure_desktop_blocks_correction() {
        let mut target = normal_target();
        target.is_secure_desktop = true;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::BlockedSecureDesktop
        );
    }

    #[test]
    fn lock_screen_blocks_correction() {
        let mut target = normal_target();
        target.is_lock_screen = true;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::BlockedLockScreen
        );
    }

    #[test]
    fn credential_dialog_blocks_correction() {
        let mut target = normal_target();
        target.is_credential_dialog = true;

        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::BlockedCredentialDialog
        );
    }

    #[test]
    fn normal_target_allows_correction() {
        assert_eq!(
            normal_target().correction_eligibility(),
            CorrectionEligibility::Allowed
        );
    }

    #[test]
    fn detection_returns_target_or_unsupported_without_panic() {
        match detect_focused_target() {
            TargetDetection::Available(target) => {
                assert_ne!(target.process_id, 0);
                assert_ne!(target.window_handle, 0);
            }
            TargetDetection::Unsupported => {}
        }
    }

    #[test]
    fn automation_id_focused_element_key_is_distinct() {
        let element_id = FocusedElementId::AutomationId("Editor".to_owned());

        assert_eq!(
            session_key_for(Some(&element_id), Some(123), Some(42), Some("Notes")),
            SessionKey::FocusedElement("automation:Editor".to_owned())
        );
    }

    #[test]
    fn com_initialization_accepts_known_success_results() {
        assert!(accept_com_initialization(S_OK));
        assert!(accept_com_initialization(S_FALSE));
    }

    #[test]
    fn com_initialization_rejects_unexpected_success_result() {
        assert!(!accept_com_initialization(HRESULT(2)));
    }

    #[test]
    fn detects_known_lock_screen_processes() {
        assert!(is_lock_screen_process("lockapp.exe"));
        assert!(is_lock_screen_process("logonui.exe"));
        assert!(!is_lock_screen_process("notepad.exe"));
    }

    #[test]
    fn detects_credential_context_from_process_or_title() {
        assert!(is_credential_context("consent.exe", ""));
        assert!(is_credential_context("notepad.exe", "Windows Security"));
        assert!(!is_credential_context("notepad.exe", "Notes"));
    }
}
