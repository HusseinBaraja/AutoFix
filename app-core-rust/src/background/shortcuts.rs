use crate::settings::{AppConfig, Shortcut, ShortcutKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutAction {
    Correct,
    Undo,
}

#[cfg(windows)]
mod native {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
        VK_F1, VK_SPACE,
    };

    use super::{Shortcut, ShortcutAction, ShortcutKey};

    const CORRECT_ID: i32 = 1;
    const UNDO_ID: i32 = 2;

    pub(crate) struct GlobalShortcutListener {
        registered: Vec<i32>,
    }

    impl GlobalShortcutListener {
        pub(crate) fn initialize(config: &crate::settings::AppConfig) -> Self {
            let mut listener = Self {
                registered: Vec::new(),
            };
            listener.reload(config);
            listener
        }

        pub(crate) fn reload(&mut self, config: &crate::settings::AppConfig) {
            self.unregister_all();
            register_one(&mut self.registered, CORRECT_ID, &config.shortcuts.correct);
            register_one(&mut self.registered, UNDO_ID, &config.shortcuts.undo);
        }

        pub(crate) fn action_for_id(id: usize) -> Option<ShortcutAction> {
            match id as i32 {
                CORRECT_ID => Some(ShortcutAction::Correct),
                UNDO_ID => Some(ShortcutAction::Undo),
                _ => None,
            }
        }

        pub(crate) fn shutdown(mut self) {
            self.unregister_all();
            tracing::info!("global shortcuts shut down");
        }

        fn unregister_all(&mut self) {
            for id in self.registered.drain(..) {
                unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), id);
                }
            }
        }
    }

    fn register_one(registered: &mut Vec<i32>, id: i32, value: &str) {
        match Shortcut::parse(value) {
            Ok(shortcut) => unsafe {
                if RegisterHotKey(
                    std::ptr::null_mut(),
                    id,
                    shortcut_modifiers(&shortcut),
                    virtual_key(&shortcut.key),
                ) == 0
                {
                    tracing::warn!("global shortcut registration failed for {}", value);
                } else {
                    registered.push(id);
                    tracing::info!("global shortcut registered for {}", value);
                }
            },
            Err(error) => tracing::warn!("global shortcut ignored: {}: {}", value, error),
        }
    }

    fn shortcut_modifiers(shortcut: &Shortcut) -> u32 {
        let mut modifiers = MOD_NOREPEAT;
        if shortcut.modifiers.ctrl {
            modifiers |= MOD_CONTROL;
        }
        if shortcut.modifiers.alt {
            modifiers |= MOD_ALT;
        }
        if shortcut.modifiers.shift {
            modifiers |= MOD_SHIFT;
        }
        if shortcut.modifiers.win {
            modifiers |= MOD_WIN;
        }
        modifiers
    }

    fn virtual_key(key: &ShortcutKey) -> u32 {
        match key {
            ShortcutKey::Space => VK_SPACE as u32,
            ShortcutKey::Letter(key) | ShortcutKey::Digit(key) => *key as u32,
            ShortcutKey::Function(number) => VK_F1 as u32 + u32::from(*number - 1),
        }
    }
}

#[cfg(not(windows))]
mod native {
    use super::ShortcutAction;

    pub(crate) struct GlobalShortcutListener;

    impl GlobalShortcutListener {
        pub(crate) fn initialize(_config: &crate::settings::AppConfig) -> Self {
            tracing::info!("global shortcuts unavailable on this platform");
            Self
        }

        pub(crate) fn reload(&mut self, _config: &crate::settings::AppConfig) {}

        pub(crate) fn action_for_id(_id: usize) -> Option<ShortcutAction> {
            None
        }

        pub(crate) fn shutdown(self) {
            tracing::info!("global shortcut placeholder shut down");
        }
    }
}

pub(crate) use native::GlobalShortcutListener;

pub(crate) fn detect_conflict(config: &AppConfig) -> bool {
    match (
        Shortcut::parse(&config.shortcuts.correct),
        Shortcut::parse(&config.shortcuts.undo),
    ) {
        (Ok(correct), Ok(undo)) => correct.conflicts_with(&undo),
        _ => false,
    }
}
