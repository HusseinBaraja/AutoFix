//! Brief manual timeout feedback, independent of the input processor and typing.

use std::sync::atomic::{AtomicBool, Ordering};

static VISIBLE: AtomicBool = AtomicBool::new(false);

/// Provide platform timeout feedback without taking focus; unsupported platforms stay silent.
pub(super) fn show() {
    if VISIBLE.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Err(error) = std::thread::Builder::new()
        .name("autofix-timeout-notice".into())
        .spawn(|| {
            native::show();
            VISIBLE.store(false, Ordering::Release);
        })
    {
        VISIBLE.store(false, Ordering::Release);
        tracing::warn!(%error, "cannot show timeout notice");
    }
}

#[cfg(windows)]
mod native {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer, SetTimer,
            ShowWindow, SystemParametersInfoW, TranslateMessage, MSG, SPI_GETWORKAREA,
            SW_SHOWNOACTIVATE, WM_TIMER, WS_BORDER, WS_DISABLED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    };

    const SS_CENTER: u32 = 0x1;
    const SS_CENTERIMAGE: u32 = 0x200;

    /// Provide platform timeout feedback without taking focus; unsupported platforms stay silent.
    pub(super) fn show() {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let text: Vec<u16> = "AutoFix: API correction timed out.\0"
            .encode_utf16()
            .collect();
        unsafe {
            let mut area: RECT = std::mem::zeroed();
            if SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) == 0 {
                tracing::warn!("cannot locate work area for timeout notice");
                return;
            }
            // Disabled, no-activate tool window: no focus, taskbar entry or input capture.
            // SS_CENTER | SS_CENTERIMAGE centers the single-line static label.
            let window = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class.as_ptr(),
                text.as_ptr(),
                WS_POPUP | WS_BORDER | WS_DISABLED | SS_CENTER | SS_CENTERIMAGE,
                area.right - 356,
                area.bottom - 64,
                340,
                48,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            );
            if window.is_null() {
                tracing::warn!("cannot create timeout notice");
                return;
            }
            let timer = SetTimer(window, 1, 2_500, None);
            if timer == 0 {
                DestroyWindow(window);
                tracing::warn!("cannot schedule timeout notice dismissal");
                return;
            }
            ShowWindow(window, SW_SHOWNOACTIVATE);
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                if message.message == WM_TIMER && message.hwnd == window && message.wParam == timer
                {
                    break;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            KillTimer(window, timer);
            DestroyWindow(window);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        ptr::null,
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetForegroundWindow, GetWindowLongW, IsWindowVisible, GWL_EXSTYLE, GWL_STYLE,
        WS_DISABLED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    /// The native notice preserves foreground focus, coalesces duplicates, and dismisses itself.
    #[test]
    #[ignore = "requires an interactive Windows desktop; briefly displays the timeout notice"]
    fn native_timeout_notice_preserves_focus_and_dismisses_itself() {
        let title: Vec<u16> = "AutoFix: API correction timed out.\0"
            .encode_utf16()
            .collect();
        let original = unsafe { GetForegroundWindow() };
        show();
        let started = Instant::now();
        let window = loop {
            let window = unsafe { FindWindowW(null(), title.as_ptr()) };
            if !window.is_null() && unsafe { IsWindowVisible(window) } != 0 {
                break window;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(10));
        };
        unsafe {
            assert_eq!(GetForegroundWindow(), original);
            assert_ne!(GetWindowLongW(window, GWL_STYLE) as u32 & WS_DISABLED, 0);
            let style = GetWindowLongW(window, GWL_EXSTYLE) as u32;
            assert_ne!(style & WS_EX_NOACTIVATE, 0);
            assert_ne!(style & WS_EX_TOOLWINDOW, 0);
        }
        // Repeated feedback coalesces into this same notice.
        show();
        while unsafe { !FindWindowW(null(), title.as_ptr()).is_null() } {
            assert!(started.elapsed() < Duration::from_secs(4));
            thread::sleep(Duration::from_millis(20));
        }
        assert!(started.elapsed() >= Duration::from_secs(2));
    }
}

#[cfg(not(windows))]
mod native {
    /// Provide platform timeout feedback without taking focus; unsupported platforms stay silent.
    pub(super) fn show() {}
}
