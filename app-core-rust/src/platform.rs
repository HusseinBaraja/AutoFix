use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextLengthW};

pub(crate) fn active_window_title_len() -> i32 {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return 0;
        }

        GetWindowTextLengthW(window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_window_title_len_is_non_negative() {
        assert!(active_window_title_len() >= 0);
    }
}
