use super::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

struct Editor(isize);
impl Editor {
    fn new(style: u32, text: &str) -> Self {
        Self::with_class("EDIT", style, text)
    }

    fn with_class(class: &str, style: u32, text: &str) -> Self {
        let window = unsafe {
            CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                style,
                0,
                0,
                300,
                80,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!window.is_null());
        Self(window as isize)
    }

    fn select(&self, start: usize, end: usize) {
        unsafe {
            SendMessageW(self.0 as _, 0x00b1, start, end as isize);
        }
    }

    fn text(&self) -> String {
        let mut buffer = [0u16; 256];
        let length = unsafe {
            SendMessageW(
                self.0 as _,
                WM_GETTEXT,
                buffer.len(),
                buffer.as_mut_ptr() as isize,
            )
        };
        String::from_utf16_lossy(&buffer[..length as usize])
    }
}

#[test]
fn native_richedit_selected_insertion_preserves_unicode_suffix_and_app_undo() {
    struct Library(windows_sys::Win32::Foundation::HMODULE);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                windows_sys::Win32::Foundation::FreeLibrary(self.0);
            }
        }
    }
    for (dll, class) in [
        ("Msftedit.dll", "RICHEDIT50W"),
        ("Riched20.dll", "RichEdit20W"),
    ] {
        let library = Library(unsafe {
            windows_sys::Win32::System::LibraryLoader::LoadLibraryW(wide(dll).as_ptr())
        });
        assert!(!library.0.is_null());
        for replacement in ["the", "é😃", "العربية", ""] {
            let editor = Editor::with_class(class, ES_MULTILINE as u32, "old teh AFTER");
            editor.select(4, 7);
            validate_selection(editor.0, "teh", replacement).unwrap();
            insert(editor.0, &prepare(editor.0, replacement).unwrap()).unwrap();
            assert_eq!(editor.text(), format!("old {replacement} AFTER"), "{class}");
            editor.select(4, 4 + replacement.encode_utf16().count());
            validate_selection(editor.0, replacement, "teh").unwrap();
            insert(editor.0, &prepare(editor.0, "teh").unwrap()).unwrap();
            assert_eq!(editor.text(), "old teh AFTER", "{class} app undo");
        }
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0 as _);
        }
    }
}

/// These tests own hidden controls. No foreground input or clipboard writes.
#[test]
fn native_selected_insertion_preserves_suffix_unicode_and_app_owned_undo() {
    for replacement in ["the", "é😃", "العربية", "", "first\r\nsecond\tline"] {
        let editor = Editor::new(ES_MULTILINE as u32, "old teh AFTER");
        let sequence =
            unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
        let prepared = prepare(editor.0, replacement).unwrap();
        editor.select(4, 7);
        validate_selection(editor.0, "teh", replacement).unwrap();
        insert(editor.0, &prepared).unwrap();
        assert_eq!(editor.text(), format!("old {replacement} AFTER"));
        assert_eq!(
            unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() },
            sequence
        );
        // Native controls own their Ctrl+Z grouping. AutoFix undo replaces only
        // its recorded range and must work even for deletion and multiline edits.
        editor.select(4, 4 + replacement.encode_utf16().count());
        validate_selection(editor.0, replacement, "teh").unwrap();
        insert(editor.0, &prepare(editor.0, "teh").unwrap()).unwrap();
        assert_eq!(editor.text(), "old teh AFTER", "app undo: {replacement:?}");
    }
}

#[test]
fn transformed_protected_disabled_and_single_line_targets_refuse_preparation() {
    for style in [
        ES_READONLY,
        ES_PASSWORD,
        ES_NUMBER,
        ES_UPPERCASE,
        ES_LOWERCASE,
    ] {
        let editor = Editor::new(style as u32, "teh");
        assert!(prepare(editor.0, "the").is_err());
        assert_eq!(
            editor.text(),
            if style == ES_UPPERCASE { "TEH" } else { "teh" }
        );
    }
    let editor = Editor::new(0, "teh");
    assert!(prepare(editor.0, "the\n").is_err());
    assert!(prepare(editor.0, "the\0").is_err());
    assert!(prepare(editor.0, &"😃".repeat(32_769)).is_err());
    unsafe {
        windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(editor.0 as _, 0);
    }
    assert!(prepare(editor.0, "the").is_err());
    assert!(!supports_target(0));
}

#[test]
fn native_limit_and_selection_mismatch_fail_before_insertion() {
    let editor = Editor::new(0, "old teh AFTER");
    unsafe {
        SendMessageW(editor.0 as _, 0x00c5, 13, 0);
    } // EM_LIMITTEXT
    editor.select(4, 7);
    assert!(validate_selection(editor.0, "teh", "longer").is_err());
    editor.select(4, 6);
    assert!(validate_selection(editor.0, "teh", "the").is_err());
    assert_eq!(editor.text(), "old teh AFTER");
}
