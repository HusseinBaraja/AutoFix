//! Synchronous selected-range insertion without the clipboard or keyboard mode.
use windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowLongPtrW, IsWindowUnicode, SendMessageTimeoutW, ES_LOWERCASE,
    ES_MULTILINE, ES_NUMBER, ES_PASSWORD, ES_READONLY, ES_UPPERCASE, GWL_STYLE, SMTO_ABORTIFHUNG,
    SMTO_BLOCK, WM_GETTEXTLENGTH,
};

const EM_REPLACESEL: u32 = 0x00c2;
const EM_GETSEL: u32 = 0x00b0;
const EM_GETLIMITTEXT: u32 = 0x00d5;

fn query(window: isize, message: u32, wparam: usize, lparam: isize) -> Result<usize, String> {
    let mut receipt = 0;
    if unsafe {
        SendMessageTimeoutW(
            window as _,
            message,
            wparam,
            lparam,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            250,
            &mut receipt,
        )
    } == 0
    {
        Err("native text capability query failed or timed out".into())
    } else {
        Ok(receipt)
    }
}

/// Do not partially truncate a replacement at the target's native length limit.
/// UIA supplies exact text proof; EM_GETSEL independently checks native offsets.
pub(super) fn validate_selection(
    window: isize,
    original: &str,
    replacement: &str,
) -> Result<(), String> {
    let mut start = 0u32;
    let mut end = 0u32;
    query(
        window,
        EM_GETSEL,
        &mut start as *mut u32 as usize,
        &mut end as *mut u32 as isize,
    )?;
    let length = query(window, WM_GETTEXTLENGTH, 0, 0)?;
    let limit = query(window, EM_GETLIMITTEXT, 0, 0)?;
    let selected = end
        .checked_sub(start)
        .ok_or("native selection offsets invalid")? as usize;
    if end as usize > length || selected != original.encode_utf16().count() {
        return Err("native selection does not match the proved range".into());
    }
    if length
        .saturating_sub(selected)
        .saturating_add(replacement.encode_utf16().count())
        > limit
    {
        return Err("replacement exceeds the native text limit".into());
    }
    Ok(())
}

/// Require a Unicode native Edit/RichEdit with ordinary, writable text semantics.
pub(super) fn supports_target(window: isize) -> bool {
    if window == 0 {
        return false;
    }
    unsafe {
        let window = window as _;
        let mut class = [0u16; 128];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
        let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
        known_class(&class)
            && IsWindowUnicode(window) != 0
            && IsWindowEnabled(window) != 0
            && style & (ES_READONLY | ES_PASSWORD | ES_NUMBER | ES_UPPERCASE | ES_LOWERCASE) as u32
                == 0
    }
}

/// Documented Unicode RichEdit classes, including Microsoft 365/modern Notepad.
fn known_class(class: &str) -> bool {
    matches!(
        class.to_ascii_lowercase().as_str(),
        "edit" | "richedit20w" | "richedit50w" | "richedit20wpt" | "richeditd2d" | "richeditd2dpt"
    )
}

/// Allocate before selection. Single-line controls cannot represent line breaks.
pub(super) fn prepare(window: isize, text: &str) -> Result<Vec<u16>, String> {
    if !supports_target(window) {
        return Err("direct insertion requires a writable Unicode Edit/RichEdit".into());
    }
    if text.contains('\0') || text.encode_utf16().count() > 65_536 {
        return Err("direct insertion contains NUL or exceeds its budget".into());
    }
    let multiline =
        unsafe { GetWindowLongPtrW(window as _, GWL_STYLE) } as u32 & ES_MULTILINE as u32 != 0;
    if !multiline && text.contains(['\r', '\n']) {
        return Err("single-line target cannot preserve line breaks".into());
    }
    let mut wide = Vec::new();
    wide.try_reserve_exact(text.encode_utf16().count() + 1)
        .map_err(|_| "direct insertion allocation failed")?;
    wide.extend(text.encode_utf16().chain(Some(0)));
    Ok(wide)
}

/// EM_REPLACESEL is below WM_USER, so Windows marshals its string across processes.
/// Keep native undo enabled. A timeout is uncertain mutation and never permits retry.
pub(super) fn insert(window: isize, text: &[u16]) -> Result<(), String> {
    let mut receipt = 0;
    if unsafe {
        SendMessageTimeoutW(
            window as _,
            EM_REPLACESEL,
            1,
            text.as_ptr() as isize,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            250,
            &mut receipt,
        )
    } == 0
    {
        Err("direct insertion failed or timed out; mutation uncertain".into())
    } else {
        // The message has no result value. UIA verifies the actual text and caret.
        Ok(())
    }
}

#[cfg(test)]
mod tests;
