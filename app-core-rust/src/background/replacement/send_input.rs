//! Last-resort keyboard mutation of an already selected, verified pre-caret span.
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, IsWindowEnabled, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_BACK, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

const MAX_UTF16_UNITS: usize = 65_536;

/// Allocate and validate the entire batch before changing the target selection.
pub(super) fn prepare(text: &str) -> Result<Vec<INPUT>, String> {
    if text.chars().any(char::is_control) {
        return Err("SendInput refuses control characters".into());
    }
    let units = text.encode_utf16().count();
    if units > MAX_UTF16_UNITS {
        return Err("SendInput exceeds the input budget".into());
    }
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(units.max(1) * 2)
        .map_err(|_| "SendInput batch allocation failed")?;
    if text.is_empty() {
        // Backspace deletes only the proven nonempty selection. Never send Delete,
        // which could consume text after the caret if a provider lost the selection.
        push_pair(&mut inputs, VK_BACK, 0, 0);
    } else {
        for unit in text.encode_utf16() {
            push_pair(&mut inputs, 0, unit, KEYEVENTF_UNICODE);
        }
    }
    Ok(inputs)
}

/// Keep each Unicode unit or virtual key adjacent to its matching release.
fn push_pair(inputs: &mut Vec<INPUT>, key: u16, scan: u16, flags: u32) {
    for up in [false, true] {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: scan,
                    dwFlags: flags | if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }
}

/// Refuse insertion through held user shortcut modifiers without releasing their keys.
pub(super) fn modifiers_held() -> bool {
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(*key)) } < 0)
}

/// Only standard Unicode Edit controls have known insert/selection semantics.
/// Custom editors and RichEdit overtype modes require a safer native strategy.
pub(super) fn supports_target(window: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindowLongPtrW, IsWindowUnicode, ES_LOWERCASE, ES_NUMBER, ES_PASSWORD,
        ES_READONLY, ES_UPPERCASE, GWL_STYLE,
    };
    if window == 0 {
        return false;
    }
    let mut class = [0u16; 16];
    unsafe {
        let window = window as _;
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
        length == 4
            && String::from_utf16_lossy(&class[..4]).eq_ignore_ascii_case("edit")
            && IsWindowUnicode(window) != 0
            && IsWindowEnabled(window) != 0
            && style & (ES_READONLY | ES_PASSWORD | ES_NUMBER | ES_UPPERCASE | ES_LOWERCASE) as u32
                == 0
    }
}

/// Submit one batch so user keystrokes cannot interleave its events. No retries:
/// even a partial batch may have deleted the selection or left a key down.
pub(super) fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if let Some(up) = pending_keyup(sent as usize, inputs) {
        // A partial batch can stop on a keydown (notably Backspace). Release
        // only that injected key; never retry deletion or Unicode keydowns.
        if unsafe { SendInput(1, up, std::mem::size_of::<INPUT>() as i32) } != 1 {
            return Err("SendInput partial batch key release failed; mutation uncertain".into());
        }
    }
    check_sent(sent as usize, inputs.len())
}

/// After a partial batch, release only the last injected keydown.
fn pending_keyup(sent: usize, inputs: &[INPUT]) -> Option<&INPUT> {
    (sent % 2 == 1).then(|| inputs.get(sent)).flatten()
}

/// Treat every incomplete input batch as uncertain mutation and never retry it.
fn check_sent(sent: usize, expected: usize) -> Result<(), String> {
    if sent == expected {
        Ok(())
    } else {
        Err(format!(
            "SendInput inserted {sent} of {expected} events; mutation uncertain"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_batch_preserves_surrogate_pairs_and_key_releases() {
        let text = "é😃 العربية";
        let inputs = prepare(text).unwrap();
        let units: Vec<_> = text.encode_utf16().collect();
        assert_eq!(inputs.len(), units.len() * 2);
        for (pair, unit) in inputs.chunks_exact(2).zip(units) {
            for (index, input) in pair.iter().enumerate() {
                assert_eq!(input.r#type, INPUT_KEYBOARD);
                let key = unsafe { input.Anonymous.ki };
                assert_eq!(key.wVk, 0);
                assert_eq!(key.wScan, unit);
                assert_eq!(
                    key.dwFlags,
                    KEYEVENTF_UNICODE | if index == 1 { KEYEVENTF_KEYUP } else { 0 }
                );
            }
        }
    }

    #[test]
    fn deletion_is_one_backspace_pair() {
        let inputs = prepare("").unwrap();
        assert_eq!(inputs.len(), 2);
        for (index, input) in inputs.iter().enumerate() {
            let key = unsafe { input.Anonymous.ki };
            assert_eq!(key.wVk, VK_BACK);
            assert_eq!(key.wScan, 0);
            assert_eq!(key.dwFlags, if index == 1 { KEYEVENTF_KEYUP } else { 0 });
        }
    }

    #[test]
    fn controls_and_oversized_unicode_payloads_refuse_before_selection() {
        for character in ['\0', '\n', '\r', '\t', '\x08', '\x1b', '\x7f', '\u{85}'] {
            assert_eq!(
                prepare(&format!("the{character}")).err().unwrap(),
                "SendInput refuses control characters"
            );
        }
        assert!(prepare(&"😃".repeat(MAX_UTF16_UNITS / 2)).is_ok());
        assert!(prepare(&"😃".repeat(MAX_UTF16_UNITS / 2 + 1)).is_err());
    }

    #[test]
    fn zero_or_partial_input_is_never_success() {
        for count in [0, 1, 5] {
            assert!(check_sent(count, 6).is_err());
        }
        assert!(check_sent(6, 6).is_ok());
    }

    #[test]
    fn partial_batch_releases_only_the_last_injected_key() {
        for text in ["", "é😃"] {
            let inputs = prepare(text).unwrap();
            for sent in 0..=inputs.len() {
                let pending = pending_keyup(sent, &inputs);
                assert_eq!(pending.is_some(), sent % 2 == 1 && sent < inputs.len());
                if let Some(up) = pending {
                    assert_ne!(unsafe { up.Anonymous.ki }.dwFlags & KEYEVENTF_KEYUP, 0);
                }
            }
        }
    }

    #[test]
    fn protected_transforming_and_unknown_controls_refuse_send_input() {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain(Some(0)).collect()
        }
        for (class, style, allowed) in [
            ("EDIT", 0, true),
            ("EDIT", ES_READONLY, false),
            ("EDIT", ES_PASSWORD, false),
            ("EDIT", ES_NUMBER, false),
            ("EDIT", ES_UPPERCASE, false),
            ("EDIT", ES_LOWERCASE, false),
            ("STATIC", 0, false),
        ] {
            let window = unsafe {
                CreateWindowExW(
                    0,
                    wide(class).as_ptr(),
                    wide("").as_ptr(),
                    style as u32,
                    0,
                    0,
                    100,
                    40,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!window.is_null());
            let supported = supports_target(window as isize);
            unsafe { DestroyWindow(window) };
            assert_eq!(supported, allowed, "class={class}, style={style}");
        }
        assert!(!supports_target(0));
    }
}
