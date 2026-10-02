//! Brief, coalesced, non-activating feedback independent of the input processor.

use std::sync::atomic::{AtomicBool, Ordering};

static VISIBLE: AtomicBool = AtomicBool::new(false);

/// Provide platform feedback without taking focus; unsupported platforms stay silent.
pub(super) fn show(text: impl Into<String>, near_caret: bool) {
    if VISIBLE.swap(true, Ordering::AcqRel) {
        return;
    }
    let position = super::super::input_listener::current_position_generation();
    let sequence = super::super::input_listener::current_input_sequence();
    let text = text.into();
    if let Err(error) = std::thread::Builder::new()
        .name("autofix-feedback-notice".into())
        .spawn(move || {
            native::show(&text, near_caret, position, sequence);
            VISIBLE.store(false, Ordering::Release);
        })
    {
        VISIBLE.store(false, Ordering::Release);
        tracing::warn!(%error, "cannot show feedback notice");
    }
}

#[cfg(windows)]
mod native {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::{
            ClientToScreen, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
            MONITOR_DEFAULTTONEAREST,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
            GetGUIThreadInfo, GetMessageW, GetWindowThreadProcessId, KillTimer, SetTimer,
            ShowWindow, SystemParametersInfoW, TranslateMessage, GUITHREADINFO, MSG,
            SPI_GETWORKAREA, SW_SHOWNOACTIVATE, WM_TIMER, WS_BORDER, WS_DISABLED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    };

    const SS_CENTER: u32 = 0x1;

    /// Provide platform feedback without taking focus; unsupported platforms stay silent.
    pub(super) fn show(text: &str, near_caret: bool, position: u64, sequence: u64) {
        let still_current = || {
            position == super::super::super::input_listener::current_position_generation()
                && sequence == super::super::super::input_listener::current_input_sequence()
        };
        if !still_current() {
            return;
        }
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let text: Vec<u16> = format!("{text}\0").encode_utf16().collect();
        unsafe {
            let mut area: RECT = std::mem::zeroed();
            if SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) == 0 {
                tracing::warn!("cannot locate work area for feedback notice");
                return;
            }
            let foreground = GetForegroundWindow();
            let mut monitor: MONITORINFO = std::mem::zeroed();
            monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(
                MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            ) != 0
            {
                area = monitor.rcWork;
            }
            let mut x = area.right - 436;
            let mut y = area.bottom - 88;
            if near_caret {
                let mut info: GUITHREADINFO = std::mem::zeroed();
                info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
                let thread = GetWindowThreadProcessId(foreground, null_mut());
                if thread != 0
                    && GetGUIThreadInfo(thread, &mut info) != 0
                    && !info.hwndCaret.is_null()
                {
                    let mut point = POINT {
                        x: info.rcCaret.left,
                        y: info.rcCaret.bottom + 8,
                    };
                    if ClientToScreen(info.hwndCaret, &mut point) != 0 {
                        x = point.x.clamp(area.left, (area.right - 420).max(area.left));
                        y = point.y.clamp(area.top, (area.bottom - 72).max(area.top));
                    }
                }
            }
            // Disabled, no-activate tool window: no focus, taskbar entry or input capture.
            // SS_CENTER wraps long previews inside the small notice.
            let window = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class.as_ptr(),
                text.as_ptr(),
                WS_POPUP | WS_BORDER | WS_DISABLED | SS_CENTER,
                x,
                y,
                420,
                72,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            );
            if window.is_null() {
                tracing::warn!("cannot create feedback notice");
                return;
            }
            let timer = SetTimer(window, 1, 100, None);
            if timer == 0 {
                DestroyWindow(window);
                tracing::warn!("cannot schedule feedback notice dismissal");
                return;
            }
            if !still_current() {
                KillTimer(window, timer);
                DestroyWindow(window);
                return;
            }
            ShowWindow(window, SW_SHOWNOACTIVATE);
            let started = std::time::Instant::now();
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                if message.message == WM_TIMER && message.hwnd == window && message.wParam == timer
                {
                    if started.elapsed() >= std::time::Duration::from_millis(2500)
                        || GetForegroundWindow() != foreground
                        || !still_current()
                    {
                        break;
                    }
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
    #[ignore = "requires an interactive Windows desktop; briefly displays a feedback notice"]
    fn native_feedback_notice_preserves_focus_and_dismisses_itself() {
        let title: Vec<u16> = "AutoFix: feedback verification.\0".encode_utf16().collect();
        let original = unsafe { GetForegroundWindow() };
        show("AutoFix: feedback verification.", true);
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
        show("AutoFix: feedback verification.", true);
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
    pub(super) fn show(_: &str, _: bool, _: u64, _: u64) {}
}
