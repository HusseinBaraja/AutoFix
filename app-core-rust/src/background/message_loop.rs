#[cfg(windows)]
mod native {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, KillTimer, PostQuitMessage, SetTimer, TranslateMessage, MSG,
        WM_HOTKEY, WM_TIMER,
    };

    const RELOAD_TIMER_ID: usize = 10;
    const INPUT_TIMER_ID: usize = 11;
    const RELOAD_TIMER_MS: u32 = 1_000;
    const INPUT_TIMER_MS: u32 = 50;

    pub(crate) enum MessageLoopEvent {
        Hotkey(usize),
        Poll,
        Tick,
    }

    pub(crate) fn run_until_exit(mut process_event: impl FnMut(MessageLoopEvent) -> bool) {
        unsafe {
            let timer_id = SetTimer(std::ptr::null_mut(), RELOAD_TIMER_ID, RELOAD_TIMER_MS, None);
            let timer_created = timer_id != 0;
            let input_timer = SetTimer(std::ptr::null_mut(), INPUT_TIMER_ID, INPUT_TIMER_MS, None);
            if input_timer == 0 {
                tracing::error!("failed to create input processing timer");
            }
            if !timer_created {
                tracing::error!(
                    "failed to create shortcut reload timer; config reload ticks disabled"
                );
            }

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
                    && message.wParam == timer_id
                    && process_event(MessageLoopEvent::Tick)
                {
                    PostQuitMessage(0);
                }

                if message.message == WM_TIMER
                    && message.wParam == input_timer
                    && process_event(MessageLoopEvent::Poll)
                {
                    PostQuitMessage(0);
                }

                TranslateMessage(&message);
                DispatchMessageW(&message);
                if process_event(MessageLoopEvent::Poll) {
                    PostQuitMessage(0);
                }
            }
            if timer_created {
                KillTimer(std::ptr::null_mut(), timer_id);
            }
            if input_timer != 0 {
                KillTimer(std::ptr::null_mut(), input_timer);
            }
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub(crate) enum MessageLoopEvent {
        Poll,
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
