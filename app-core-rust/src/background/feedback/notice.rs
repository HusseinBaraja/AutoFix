//! Brief, coalesced, non-activating feedback independent of the input processor.

use crate::background::{
    input_listener,
    pipeline::InputStamp,
    target::{self, CorrectionEligibility, FocusedTarget, TargetDetection},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

static VISIBLE: AtomicBool = AtomicBool::new(false);

/// Immutable display authority; never recapture a preview's origin on its worker.
pub(super) struct Origin {
    stamp: InputStamp,
    foreground: isize,
    target: Option<FocusedTarget>,
    cancelled: Arc<AtomicBool>,
    policy_cancelled: Arc<AtomicBool>,
}

impl Origin {
    /// Generic notices contain no document text but still expire with input and policy.
    pub(super) fn current(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            stamp: Self::input_stamp(),
            foreground: target::active_window_handle_value(),
            target: None,
            policy_cancelled: Arc::clone(&cancelled),
            cancelled,
        }
    }

    /// Carry the target and generations validated by the correction pipeline.
    pub(super) fn validated(
        stamp: InputStamp,
        target: FocusedTarget,
        cancelled: Arc<AtomicBool>,
        policy_cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            stamp,
            foreground: target.window_handle,
            target: Some(target),
            cancelled,
            policy_cancelled,
        }
    }

    fn input_stamp() -> InputStamp {
        InputStamp {
            position: input_listener::current_position_generation(),
            sequence: input_listener::current_input_sequence(),
        }
    }

    fn input_is_current(&self, stamp: InputStamp, foreground: isize) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && !self.policy_cancelled.load(Ordering::Acquire)
            && self.stamp == stamp
            && self.foreground != 0
            && self.foreground == foreground
    }

    fn matches(&self, stamp: InputStamp, foreground: isize, live: Option<&FocusedTarget>) -> bool {
        self.input_is_current(stamp, foreground)
            && self.target.as_ref().is_none_or(|origin| {
                live.is_some_and(|live| {
                    live.correction_eligibility() == CorrectionEligibility::Allowed
                        && live.focused_element_id.is_some()
                        && live == origin
                })
            })
    }

    /// Check before display and throughout its lifetime, including UIA focus within a window.
    fn is_current(&self) -> bool {
        if !self.input_is_current(Self::input_stamp(), target::active_window_handle_value()) {
            return false;
        }
        let live = if self.target.is_some() {
            match target::detect_focused_target() {
                TargetDetection::Available(target) => Some(target),
                TargetDetection::Unsupported => None,
            }
        } else {
            None
        };
        // Target detection itself can race with typing, focus changes or config reset.
        self.matches(
            Self::input_stamp(),
            target::active_window_handle_value(),
            live.as_ref(),
        )
    }
}

/// Provide platform feedback without taking focus; unsupported platforms stay silent.
pub(super) fn show(text: impl Into<String>, near_caret: bool, origin: Origin) {
    if VISIBLE.swap(true, Ordering::AcqRel) {
        return;
    }
    let text = text.into();
    if let Err(error) = std::thread::Builder::new()
        .name("autofix-feedback-notice".into())
        .spawn(move || {
            native::show(&text, near_caret, origin);
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
    pub(super) fn show(text: &str, near_caret: bool, origin: super::Origin) {
        let still_current = || origin.is_current();
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
            let foreground = origin.foreground as windows_sys::Win32::Foundation::HWND;
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
                if message.message == WM_TIMER
                    && message.hwnd == window
                    && message.wParam == timer
                    && (started.elapsed() >= std::time::Duration::from_millis(2500)
                        || GetForegroundWindow() != foreground
                        || !still_current())
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

#[cfg(test)]
mod origin_tests {
    use super::*;
    use crate::background::target::FocusedElementId;

    fn target() -> FocusedTarget {
        FocusedTarget {
            process_id: 1,
            process_name: "editor.exe".into(),
            window_handle: 2,
            window_title: "Notes".into(),
            focused_element_id: Some(FocusedElementId::RuntimeId("text".into())),
            is_elevated: false,
            is_password_or_protected: false,
            is_hidden_or_unavailable: false,
            field_safety_known: true,
            is_secure_desktop: false,
            is_lock_screen: false,
            is_credential_dialog: false,
        }
    }

    /// Delayed display and visible previews require the original input and target.
    #[test]
    fn previews_never_rebind_to_new_input_or_target() {
        let stamp = InputStamp {
            position: 3,
            sequence: 4,
        };
        let target = target();
        let origin = Origin::validated(
            stamp,
            target.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(origin.matches(stamp, 2, Some(&target)));
        assert!(!origin.matches(
            InputStamp {
                position: 4,
                ..stamp
            },
            2,
            Some(&target)
        ));
        assert!(!origin.matches(
            InputStamp {
                sequence: 5,
                ..stamp
            },
            2,
            Some(&target)
        ));
        assert!(!origin.matches(stamp, 3, Some(&target)));
        assert!(!origin.matches(stamp, 0, Some(&target)));
        assert!(!origin.matches(stamp, 2, None));
        for change in 0..12 {
            let mut live = target.clone();
            match change {
                0 => live.focused_element_id = Some(FocusedElementId::RuntimeId("password".into())),
                1 => live.focused_element_id = None,
                2 => live.window_title = "Other document".into(),
                3 => live.process_id += 1,
                4 => live.process_name = "other.exe".into(),
                5 => live.window_handle += 1,
                6 => live.is_password_or_protected = true,
                7 => live.is_secure_desktop = true,
                8 => live.field_safety_known = false,
                9 => live.is_elevated = true,
                10 => live.is_hidden_or_unavailable = true,
                _ => live.is_credential_dialog = true,
            }
            assert!(
                !origin.matches(stamp, 2, Some(&live)),
                "target change {change}"
            );
        }
    }

    /// Cancellation remains attached after dispatch, independently of input generations.
    #[test]
    fn cancellation_and_policy_reset_revoke_dispatched_previews() {
        let stamp = InputStamp {
            position: 3,
            sequence: 4,
        };
        for policy_reset in [false, true] {
            let mut feedback = super::super::Feedback::default();
            let cancelled = Arc::new(AtomicBool::new(false));
            let target = target();
            let origin = Origin::validated(
                stamp,
                target.clone(),
                Arc::clone(&cancelled),
                Arc::clone(&feedback.notice_cancelled),
            );
            assert!(origin.matches(stamp, 2, Some(&target)));
            if policy_reset {
                feedback.reset();
                assert!(!feedback.notice_cancelled.load(Ordering::Acquire));
            } else {
                cancelled.store(true, Ordering::Release);
            }
            assert!(!origin.matches(stamp, 2, Some(&target)));
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
        let cancelled = Arc::new(AtomicBool::new(false));
        show(
            "AutoFix: feedback verification.",
            true,
            Origin::current(Arc::clone(&cancelled)),
        );
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
        show(
            "AutoFix: feedback verification.",
            true,
            Origin::current(cancelled),
        );
        while unsafe { !FindWindowW(null(), title.as_ptr()).is_null() } {
            assert!(started.elapsed() < Duration::from_secs(4));
            thread::sleep(Duration::from_millis(20));
        }
        assert!(started.elapsed() >= Duration::from_secs(2));

        // A reset after dispatch dismisses the visible window before its normal expiry.
        let cancelled = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        while VISIBLE.load(Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(1));
            thread::sleep(Duration::from_millis(10));
        }
        show(
            "AutoFix: feedback verification.",
            false,
            Origin::current(Arc::clone(&cancelled)),
        );
        loop {
            let window = unsafe { FindWindowW(null(), title.as_ptr()) };
            if !window.is_null() && unsafe { IsWindowVisible(window) } != 0 {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(1));
            thread::sleep(Duration::from_millis(10));
        }
        cancelled.store(true, Ordering::Release);
        while VISIBLE.load(Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(1));
            thread::sleep(Duration::from_millis(10));
        }
        assert!(unsafe { FindWindowW(null(), title.as_ptr()).is_null() });

        // Revoked work cannot create a window, even if the worker starts later.
        show(
            "AutoFix: feedback verification.",
            false,
            Origin::current(cancelled),
        );
        while VISIBLE.load(Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(1));
            thread::sleep(Duration::from_millis(10));
        }
        assert!(unsafe { FindWindowW(null(), title.as_ptr()).is_null() });
    }
}

#[cfg(not(windows))]
mod native {
    /// Provide platform timeout feedback without taking focus; unsupported platforms stay silent.
    pub(super) fn show(_: &str, _: bool, _: super::Origin) {}
}
