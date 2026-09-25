use super::typing::{MovementSignal, TypedInput};
use std::sync::atomic::{AtomicU64, Ordering};

static POSITION_GENERATION: AtomicU64 = AtomicU64::new(0);
static INPUT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn current_position_generation() -> u64 {
    POSITION_GENERATION.load(Ordering::Acquire)
}

pub(crate) fn current_input_sequence() -> u64 {
    INPUT_SEQUENCE.load(Ordering::Acquire)
}

fn mark_position_change() {
    POSITION_GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(crate) enum InputEvent {
    Key(KeyStroke),
    MouseClick,
    FocusChange,
}

pub(crate) struct KeyStroke {
    pub(crate) window: isize,
    pub(crate) position_generation: u64,
    pub(crate) input_sequence: u64,
    virtual_key: u32,
    scan_code: u32,
    shift: bool,
    control: bool,
    alt: bool,
    altgr: bool,
    win: bool,
    caps_lock: bool,
}

impl KeyStroke {
    pub(crate) fn translate(self) -> TypedInput {
        native::translate(self)
    }
}

#[cfg(test)]
pub(crate) fn stale_key_for_test() -> KeyStroke {
    KeyStroke {
        window: 1,
        position_generation: current_position_generation().wrapping_sub(1),
        input_sequence: current_input_sequence(),
        virtual_key: 0,
        scan_code: 0,
        shift: false,
        control: false,
        alt: false,
        altgr: false,
        win: false,
        caps_lock: false,
    }
}

#[cfg(windows)]
mod native {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Mutex, OnceLock,
        },
        thread::{self, JoinHandle},
    };

    use windows_sys::Win32::{
        Foundation::ERROR_GEN_FAILURE,
        System::Threading::GetCurrentThreadId,
        UI::{
            Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VK_BACK, VK_CAPITAL,
                VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_HOME, VK_LCONTROL, VK_LEFT, VK_LMENU,
                VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_NUMLOCK, VK_PRIOR, VK_RCONTROL, VK_RETURN,
                VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SHIFT, VK_UP,
            },
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW,
                GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
                TranslateMessage, UnhookWindowsHookEx, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND,
                HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
                PM_NOREMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT, WM_KEYDOWN,
                WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
                WM_XBUTTONDOWN,
            },
        },
    };

    use super::{
        current_position_generation, mark_position_change, InputEvent, KeyStroke, MovementSignal,
        TypedInput, INPUT_SEQUENCE,
    };

    const QUEUE_LIMIT: usize = 512;
    static EVENTS: OnceLock<Mutex<VecDeque<RawEvent>>> = OnceLock::new();
    static OVERFLOWED: AtomicBool = AtomicBool::new(false);

    enum RawEvent {
        Key(KeyStroke),
        MouseClick,
        FocusChange,
    }

    pub(crate) struct InputListener {
        thread_id: u32,
        thread: Option<JoinHandle<()>>,
    }

    struct Hooks {
        keyboard: HHOOK,
        mouse: HHOOK,
        foreground: HWINEVENTHOOK,
        focus: HWINEVENTHOOK,
    }

    impl InputListener {
        pub(crate) fn initialize() -> Result<Self, u32> {
            EVENTS.get_or_init(|| Mutex::new(VecDeque::new()));
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let thread = thread::Builder::new()
                .name("autofix-input-hooks".into())
                .spawn(move || {
                    let thread_id = unsafe { GetCurrentThreadId() };
                    let mut message = unsafe { std::mem::zeroed::<MSG>() };
                    // PostThreadMessage requires the destination message queue to exist.
                    unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
                    let hooks = Hooks::install();
                    let ready = hooks.as_ref().map(|_| thread_id).map_err(|error| *error);
                    if ready_tx.send(ready).is_err() {
                        return;
                    }
                    let Ok(_hooks) = hooks else {
                        return;
                    };
                    loop {
                        let result =
                            unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
                        if result <= 0 {
                            if result < 0 {
                                tracing::error!(
                                    error =
                                        unsafe { windows_sys::Win32::Foundation::GetLastError() },
                                    "keyboard session listener message loop failed"
                                );
                            }
                            break;
                        }
                        unsafe {
                            TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                })
                .map_err(|error| {
                    error
                        .raw_os_error()
                        .map_or(ERROR_GEN_FAILURE, |code| code as u32)
                })?;
            match ready_rx.recv() {
                Ok(Ok(thread_id)) => {
                    tracing::info!("keyboard session listener initialized");
                    Ok(Self {
                        thread_id,
                        thread: Some(thread),
                    })
                }
                result => {
                    let _ = thread.join();
                    Err(result
                        .ok()
                        .and_then(Result::err)
                        .unwrap_or(ERROR_GEN_FAILURE))
                }
            }
        }

        pub(crate) fn drain(&self) -> Vec<InputEvent> {
            let mut raw = VecDeque::new();
            if let Ok(mut events) = queue().lock() {
                std::mem::swap(&mut *events, &mut raw);
            } else {
                OVERFLOWED.store(true, Ordering::Relaxed);
                mark_position_change();
            }
            let mut result = Vec::new();
            if OVERFLOWED.swap(false, Ordering::Relaxed) {
                return vec![InputEvent::FocusChange];
            }
            for event in raw {
                match event {
                    RawEvent::Key(key) => result.push(InputEvent::Key(key)),
                    RawEvent::MouseClick => {
                        result.clear();
                        result.push(InputEvent::MouseClick);
                    }
                    RawEvent::FocusChange => {
                        result.clear();
                        result.push(InputEvent::FocusChange);
                    }
                }
            }
            result
        }

        #[cfg(test)]
        pub(crate) fn hook_thread_id(&self) -> u32 {
            self.thread_id
        }
    }

    impl Hooks {
        fn install() -> Result<Self, u32> {
            let keyboard = unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), std::ptr::null_mut(), 0)
            };
            if keyboard.is_null() {
                return Err(unsafe { windows_sys::Win32::Foundation::GetLastError() });
            }
            let mouse = unsafe {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), std::ptr::null_mut(), 0)
            };
            let foreground = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    std::ptr::null_mut(),
                    Some(focus_hook),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            let focus = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_FOCUS,
                    EVENT_OBJECT_FOCUS,
                    std::ptr::null_mut(),
                    Some(focus_hook),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if mouse.is_null() || foreground.is_null() || focus.is_null() {
                let error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                unsafe {
                    UnhookWindowsHookEx(keyboard);
                    if !mouse.is_null() {
                        UnhookWindowsHookEx(mouse);
                    }
                    if !foreground.is_null() {
                        UnhookWinEvent(foreground);
                    }
                    if !focus.is_null() {
                        UnhookWinEvent(focus);
                    }
                }
                return Err(error);
            }
            Ok(Self {
                keyboard,
                mouse,
                foreground,
                focus,
            })
        }
    }

    impl Drop for InputListener {
        fn drop(&mut self) {
            if self
                .thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
                && unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) } == 0
            {
                tracing::error!("failed to stop keyboard session listener thread");
                return;
            }
            if let Some(thread) = self.thread.take() {
                if thread.join().is_err() {
                    tracing::error!("keyboard session listener thread panicked");
                }
            }
            tracing::info!("keyboard session listener shut down");
        }
    }

    impl Drop for Hooks {
        fn drop(&mut self) {
            unsafe {
                UnhookWindowsHookEx(self.keyboard);
                UnhookWindowsHookEx(self.mouse);
                UnhookWinEvent(self.foreground);
                UnhookWinEvent(self.focus);
            }
            if let Ok(mut events) = queue().lock() {
                events.clear();
            }
        }
    }

    fn queue() -> &'static Mutex<VecDeque<RawEvent>> {
        EVENTS.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    fn push(event: RawEvent) {
        if let Ok(mut events) = queue().lock() {
            if events.len() == QUEUE_LIMIT {
                events.clear();
                OVERFLOWED.store(true, Ordering::Relaxed);
                mark_position_change();
            }
            events.push_back(event);
        } else {
            OVERFLOWED.store(true, Ordering::Relaxed);
            mark_position_change();
        }
    }

    unsafe extern "system" fn keyboard_hook(code: i32, message: usize, data: isize) -> isize {
        if code >= 0 && (message as u32 == WM_KEYDOWN || message as u32 == WM_SYSKEYDOWN) {
            let key = &*(data as *const KBDLLHOOKSTRUCT);
            if key.flags & LLKHF_INJECTED == 0 {
                push(RawEvent::Key(KeyStroke {
                    window: GetForegroundWindow() as isize,
                    position_generation: current_position_generation(),
                    input_sequence: INPUT_SEQUENCE
                        .fetch_add(1, Ordering::AcqRel)
                        .wrapping_add(1),
                    virtual_key: key.vkCode,
                    scan_code: key.scanCode,
                    shift: held(VK_SHIFT),
                    control: held(VK_CONTROL),
                    alt: held(VK_MENU),
                    altgr: held(VK_RMENU),
                    win: held(VK_LWIN) || held(VK_RWIN),
                    caps_lock: GetKeyState(VK_CAPITAL as i32) & 1 != 0,
                }));
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, message, data)
    }

    unsafe extern "system" fn mouse_hook(code: i32, message: usize, data: isize) -> isize {
        if code >= 0
            && matches!(
                message as u32,
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
            )
        {
            let mouse = &*(data as *const MSLLHOOKSTRUCT);
            if mouse.flags & LLMHF_INJECTED == 0 {
                mark_position_change();
                push(RawEvent::MouseClick);
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, message, data)
    }

    unsafe extern "system" fn focus_hook(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _window: windows_sys::Win32::Foundation::HWND,
        _object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        mark_position_change();
        push(RawEvent::FocusChange);
    }

    unsafe fn held(key: u16) -> bool {
        GetAsyncKeyState(key as i32) < 0
    }

    pub(super) fn translate(key: KeyStroke) -> TypedInput {
        if key.virtual_key > 255 {
            return TypedInput::Uncertain(MovementSignal::UnknownPosition);
        }
        let vk = key.virtual_key as u16;
        if key.control && matches!(vk, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN) {
            return TypedInput::Uncertain(MovementSignal::ControlArrow);
        }
        if key.shift && matches!(vk, VK_LEFT | VK_RIGHT) {
            return TypedInput::Uncertain(MovementSignal::UnknownPosition);
        }
        match vk {
            VK_UP | VK_DOWN => return TypedInput::Uncertain(MovementSignal::VerticalArrow),
            VK_HOME | VK_END => return TypedInput::Uncertain(MovementSignal::HomeEnd),
            VK_PRIOR | VK_NEXT => return TypedInput::Uncertain(MovementSignal::Page),
            VK_BACK if !key.control && !key.alt && !key.win => return TypedInput::Backspace,
            VK_DELETE if !key.control && !key.alt && !key.win && !key.shift => {
                return TypedInput::Delete
            }
            VK_LEFT if !key.alt && !key.win => return TypedInput::Left,
            VK_RIGHT if !key.alt && !key.win => return TypedInput::Right,
            VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU
            | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN | VK_CAPITAL | VK_NUMLOCK | VK_SCROLL => {
                return TypedInput::Text(String::new())
            }
            _ => {}
        }
        if key.win || key.control && !key.altgr || key.alt && !key.altgr {
            return TypedInput::Uncertain(MovementSignal::UnknownPosition);
        }
        let mut state = [0u8; 256];
        state[vk as usize] = 0x80;
        if key.shift {
            state[VK_SHIFT as usize] = 0x80;
        }
        if key.control {
            state[VK_CONTROL as usize] = 0x80;
        }
        if key.alt {
            state[VK_MENU as usize] = 0x80;
        }
        if key.caps_lock {
            state[VK_CAPITAL as usize] = 1;
        }
        let mut utf16 = [0u16; 8];
        let thread = unsafe { GetWindowThreadProcessId(key.window as _, std::ptr::null_mut()) };
        let layout = unsafe { GetKeyboardLayout(thread) };
        // Flag 4 leaves the target thread's dead-key state untouched.
        let len = unsafe {
            ToUnicodeEx(
                key.virtual_key,
                key.scan_code,
                state.as_ptr(),
                utf16.as_mut_ptr(),
                utf16.len() as i32,
                4,
                layout,
            )
        };
        if len <= 0 || len as usize > utf16.len() {
            return TypedInput::Uncertain(MovementSignal::UnknownPosition);
        }
        let text = String::from_utf16(&utf16[..len as usize])
            .unwrap_or_default()
            .replace('\r', "\n");
        if text.is_empty() || (vk != VK_RETURN && text.chars().any(char::is_control)) {
            TypedInput::Uncertain(MovementSignal::UnknownPosition)
        } else {
            TypedInput::Text(text)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn key(virtual_key: u16) -> KeyStroke {
            KeyStroke {
                window: 0,
                position_generation: 0,
                input_sequence: 0,
                virtual_key: virtual_key.into(),
                scan_code: 0,
                shift: false,
                control: false,
                alt: false,
                altgr: false,
                win: false,
                caps_lock: false,
            }
        }

        #[test]
        fn maps_navigation_to_uncertainty_signals() {
            for vk in [VK_UP, VK_DOWN] {
                assert_eq!(
                    translate(key(vk)),
                    TypedInput::Uncertain(MovementSignal::VerticalArrow)
                );
            }
            for vk in [VK_HOME, VK_END] {
                assert_eq!(
                    translate(key(vk)),
                    TypedInput::Uncertain(MovementSignal::HomeEnd)
                );
            }
            for vk in [VK_PRIOR, VK_NEXT] {
                assert_eq!(
                    translate(key(vk)),
                    TypedInput::Uncertain(MovementSignal::Page)
                );
            }
            let mut control_left = key(VK_LEFT);
            control_left.control = true;
            assert_eq!(
                translate(control_left),
                TypedInput::Uncertain(MovementSignal::ControlArrow)
            );
        }

        #[test]
        fn maps_editing_keys_and_ignores_modifiers() {
            assert_eq!(translate(key(VK_BACK)), TypedInput::Backspace);
            assert_eq!(translate(key(VK_DELETE)), TypedInput::Delete);
            assert_eq!(translate(key(VK_LEFT)), TypedInput::Left);
            assert_eq!(translate(key(VK_RIGHT)), TypedInput::Right);
            assert_eq!(translate(key(VK_LSHIFT)), TypedInput::Text(String::new()));
            let mut selection = key(VK_RIGHT);
            selection.shift = true;
            assert_eq!(
                translate(selection),
                TypedInput::Uncertain(MovementSignal::UnknownPosition)
            );
            let mut cut = key(VK_DELETE);
            cut.shift = true;
            assert_eq!(
                translate(cut),
                TypedInput::Uncertain(MovementSignal::UnknownPosition)
            );
        }
    }
}

#[cfg(not(windows))]
mod native {
    use super::{InputEvent, KeyStroke, MovementSignal, TypedInput};

    pub(crate) struct InputListener;
    impl InputListener {
        pub(crate) fn initialize() -> Result<Self, u32> {
            Ok(Self)
        }
        pub(crate) fn drain(&self) -> Vec<InputEvent> {
            Vec::new()
        }
    }

    pub(super) fn translate(_key: KeyStroke) -> TypedInput {
        TypedInput::Uncertain(MovementSignal::UnknownPosition)
    }
}

pub(crate) use native::InputListener;
