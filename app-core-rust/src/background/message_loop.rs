#[cfg(windows)]
mod native {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, KillTimer, PostQuitMessage, SetTimer, TranslateMessage, MSG,
        WM_HOTKEY, WM_TIMER,
    };

    const RELOAD_TIMER_ID: usize = 10;
    const RELOAD_TIMER_MS: u32 = 1_000;

    pub(crate) enum MessageLoopEvent {
        Hotkey(usize),
        Tick,
    }

    pub(crate) fn run_until_exit(mut process_event: impl FnMut(MessageLoopEvent) -> bool) {
        unsafe {
            SetTimer(std::ptr::null_mut(), RELOAD_TIMER_ID, RELOAD_TIMER_MS, None);
            let mut message = std::mem::zeroed::<MSG>();
            loop {
                let result = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
                if result <= 0 {
                    break;
                }

                if message.message == WM_HOTKEY {
                    if process_event(MessageLoopEvent::Hotkey(message.wParam)) {
                        PostQuitMessage(0);
                    }
                    continue;
                }
                if message.message == WM_TIMER
                    && message.wParam == RELOAD_TIMER_ID
                    && process_event(MessageLoopEvent::Tick)
                {
                    PostQuitMessage(0);
                }

                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            KillTimer(std::ptr::null_mut(), RELOAD_TIMER_ID);
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub(crate) enum MessageLoopEvent {
        Hotkey(usize),
        Tick,
    }

    pub(crate) fn run_until_exit(mut process_event: impl FnMut(MessageLoopEvent) -> bool) {
        loop {
            if process_event(MessageLoopEvent::Tick) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
}

pub(crate) use native::{run_until_exit, MessageLoopEvent};
