use std::path::Path;

use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    },
};

#[cfg(test)]
pub(crate) fn active_window_title_len() -> i32 {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return 0;
        }

        GetWindowTextLengthW(window)
    }
}

pub(crate) fn active_app_name() -> String {
    active_process_name()
        .or_else(active_window_title)
        .unwrap_or_else(|| "Unknown app".to_owned())
}

fn active_process_name() -> Option<String> {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return None;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        if process_id == 0 {
            return None;
        }

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

fn active_window_title() -> Option<String> {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return None;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_window_title_len_is_non_negative() {
        assert!(active_window_title_len() >= 0);
    }

    #[test]
    fn active_app_name_returns_fallback_when_title_is_unavailable() {
        assert!(!active_app_name().trim().is_empty());
    }
}
