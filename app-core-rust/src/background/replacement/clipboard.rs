//! Preserve every available format before replacing clipboard contents.
//! Unsupported owner-managed formats refuse the path before EmptyClipboard.
#[cfg(windows)]
mod windows_clipboard {
    use std::{
        ptr, thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, GlobalFree, SetLastError, HANDLE, HWND},
        Graphics::Gdi::{CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteMetaFile, DeleteObject},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
                GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
                SetClipboardData, METAFILEPICT,
            },
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::OleDuplicateData,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetClassNameW, SendMessageTimeoutW, HWND_MESSAGE,
            SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_PASTE,
        },
    };

    const UNICODE_TEXT: u32 = 13;
    const BITMAP: u32 = 2;
    const METAFILE: u32 = 3;
    const PALETTE: u32 = 9;
    const ENH_METAFILE: u32 = 14;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    struct ClipboardLock;
    impl ClipboardLock {
        fn open(owner: HWND) -> Result<Self, String> {
            let deadline = Instant::now() + Duration::from_millis(250);
            loop {
                if unsafe { OpenClipboard(owner) } != 0 {
                    return Ok(Self);
                }
                if Instant::now() >= deadline {
                    return Err("clipboard is busy".into());
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
    impl Drop for ClipboardLock {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    struct SavedFormat {
        format: u32,
        handle: HANDLE,
    }
    impl SavedFormat {
        fn duplicate(format: u32, source: HANDLE) -> Result<Self, String> {
            // Private/GDI-owner and owner-display data cannot be restored by a new owner.
            if source.is_null() || format == 0x80 || (0x200..=0x3ff).contains(&format) {
                return Err(format!("clipboard format {format} cannot be preserved"));
            }
            let native_format = match format {
                0x82 => BITMAP,
                0x83 => METAFILE,
                0x8e => ENH_METAFILE,
                other => other,
            };
            let handle = unsafe {
                if native_format == ENH_METAFILE {
                    CopyEnhMetaFileW(source, ptr::null())
                } else {
                    OleDuplicateData(source, native_format as u16, GMEM_MOVEABLE)
                }
            };
            if handle.is_null() {
                return Err(format!("clipboard format {format} could not be duplicated"));
            }
            Ok(Self { format, handle })
        }
        fn bytes(format: u32, bytes: &[u8]) -> Result<Self, String> {
            let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
            if handle.is_null() {
                return Err("clipboard allocation failed".into());
            }
            let value = Self { format, handle };
            let memory = unsafe { GlobalLock(handle) };
            if memory.is_null() {
                return Err("clipboard memory lock failed".into());
            }
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), memory.cast(), bytes.len());
                GlobalUnlock(handle);
            }
            Ok(value)
        }
        fn publish(&mut self) -> Result<(), String> {
            if unsafe { SetClipboardData(self.format, self.handle) }.is_null() {
                return Err(format!(
                    "clipboard format {} could not be restored",
                    self.format
                ));
            }
            self.handle = ptr::null_mut(); // Ownership transferred to Windows.
            Ok(())
        }
    }
    impl Drop for SavedFormat {
        fn drop(&mut self) {
            if self.handle.is_null() {
                return;
            }
            unsafe {
                match self.format {
                    BITMAP | PALETTE | 0x82 => {
                        DeleteObject(self.handle);
                    }
                    ENH_METAFILE | 0x8e => {
                        DeleteEnhMetaFile(self.handle);
                    }
                    METAFILE | 0x83 => {
                        let picture = GlobalLock(self.handle).cast::<METAFILEPICT>();
                        if !picture.is_null() {
                            DeleteMetaFile((*picture).hMF);
                            GlobalUnlock(self.handle);
                        }
                        GlobalFree(self.handle);
                    }
                    _ => {
                        GlobalFree(self.handle);
                    }
                }
            }
        }
    }

    pub(in crate::background::replacement) struct ClipboardTransaction {
        owner: HWND,
        saved: Vec<SavedFormat>,
        sequence: u32,
        changed: bool,
    }
    pub(in crate::background::replacement) struct PreparationFailure {
        pub(in crate::background::replacement) reason: String,
        pub(in crate::background::replacement) clipboard_uncertain: bool,
    }
    impl From<String> for PreparationFailure {
        fn from(reason: String) -> Self {
            Self {
                reason,
                clipboard_uncertain: false,
            }
        }
    }
    impl From<&str> for PreparationFailure {
        fn from(reason: &str) -> Self {
            reason.to_owned().into()
        }
    }
    impl ClipboardTransaction {
        pub(in crate::background::replacement) fn begin(
            text: &str,
        ) -> Result<Self, PreparationFailure> {
            let owner = unsafe {
                CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide("AutoFix clipboard").as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                )
            };
            if owner.is_null() {
                return Err("clipboard owner window unavailable".into());
            }
            let mut transaction = Self {
                owner,
                saved: Vec::new(),
                sequence: 0,
                changed: false,
            };
            let lock = ClipboardLock::open(owner)?;
            let mut format = 0;
            loop {
                unsafe {
                    SetLastError(0);
                }
                format = unsafe { EnumClipboardFormats(format) };
                if format == 0 {
                    if unsafe { GetLastError() } != 0 {
                        return Err("clipboard enumeration failed".into());
                    }
                    break;
                }
                transaction
                    .saved
                    .push(SavedFormat::duplicate(format, unsafe {
                        GetClipboardData(format)
                    })?);
            }
            // Allocate everything before EmptyClipboard so allocation failure is harmless.
            let text: Vec<u8> = text
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_ne_bytes)
                .collect();
            let mut temporary = vec![SavedFormat::bytes(UNICODE_TEXT, &text)?];
            for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
                let format = unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) };
                if format == 0 {
                    return Err("clipboard privacy format unavailable".into());
                }
                temporary.push(SavedFormat::bytes(format, &0u32.to_ne_bytes())?);
            }
            if unsafe { EmptyClipboard() } == 0 {
                return Err("clipboard could not be prepared".into());
            }
            transaction.changed = true;
            transaction.sequence = unsafe { GetClipboardSequenceNumber() };
            for value in &mut temporary {
                if let Err(error) = value.publish() {
                    // We still hold the clipboard; restore directly before returning.
                    let restored = transaction.restore_locked();
                    transaction.sequence = unsafe { GetClipboardSequenceNumber() };
                    let clipboard_uncertain = restored.is_err();
                    return Err(PreparationFailure {
                        reason: format!(
                            "{error}; {}",
                            restored
                                .err()
                                .unwrap_or_else(|| "clipboard restored".into())
                        ),
                        clipboard_uncertain,
                    });
                }
            }
            transaction.sequence = unsafe { GetClipboardSequenceNumber() };
            drop(lock);
            Ok(transaction)
        }
        fn restore_locked(&mut self) -> Result<(), String> {
            // Keep originals until the whole restore succeeds, including retry paths.
            let mut copies = self
                .saved
                .iter()
                .map(|value| SavedFormat::duplicate(value.format, value.handle))
                .collect::<Result<Vec<_>, _>>()?;
            if unsafe { EmptyClipboard() } == 0 {
                return Err("clipboard restore failed".into());
            }
            self.sequence = unsafe { GetClipboardSequenceNumber() };
            let mut failure = None;
            for value in &mut copies {
                if let Err(reason) = value.publish() {
                    failure = Some(reason);
                }
            }
            self.sequence = unsafe { GetClipboardSequenceNumber() };
            if let Some(reason) = failure {
                return Err(reason);
            }
            self.changed = false;
            Ok(())
        }
        pub(in crate::background::replacement) fn restore(&mut self) -> Result<(), String> {
            if !self.changed {
                return Ok(());
            }
            let _lock = ClipboardLock::open(self.owner)?;
            if unsafe { GetClipboardSequenceNumber() } != self.sequence {
                self.changed = false;
                return Err("clipboard changed externally; newer clipboard retained".into());
            }
            self.restore_locked()
        }
    }
    impl Drop for ClipboardTransaction {
        fn drop(&mut self) {
            if let Err(reason) = self.restore() {
                tracing::error!(reason, "clipboard restore failed");
            }
            unsafe {
                DestroyWindow(self.owner);
            }
        }
    }

    /// WM_PASTE is supported only by known native edit controls. Other apps use SendInput.
    pub(in crate::background::replacement) fn supports_paste(window: isize) -> bool {
        if window == 0 {
            return false;
        }
        let mut class = [0u16; 128];
        let length =
            unsafe { GetClassNameW(window as HWND, class.as_mut_ptr(), class.len() as i32) };
        let class = String::from_utf16_lossy(&class[..length.max(0) as usize]).to_ascii_lowercase();
        matches!(
            class.as_str(),
            "edit" | "richedit20w" | "richedit50w" | "richedit20a" | "richedit"
        )
    }
    pub(in crate::background::replacement) fn paste(window: isize) -> Result<(), String> {
        let mut result = 0;
        if unsafe {
            SendMessageTimeoutW(
                window as HWND,
                WM_PASTE,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                250,
                &mut result,
            )
        } == 0
        {
            Err("clipboard paste failed or timed out; mutation uncertain".into())
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn clipboard_format_snapshot_owns_all_bytes_independently() {
            let bytes = b"{\\rtf1 rich text}\0\xff\x01";
            let source = SavedFormat::bytes(0xc001, bytes).unwrap();
            let copy = SavedFormat::duplicate(source.format, source.handle).unwrap();
            drop(source);
            unsafe {
                let memory = GlobalLock(copy.handle);
                assert!(!memory.is_null());
                assert_eq!(
                    std::slice::from_raw_parts(memory.cast::<u8>(), bytes.len()),
                    bytes
                );
                GlobalUnlock(copy.handle);
            }
        }

        #[test]
        fn owner_managed_clipboard_formats_refuse_duplication() {
            let source = SavedFormat::bytes(UNICODE_TEXT, b"data\0").unwrap();
            for format in [0x80, 0x200, 0x2ff, 0x300, 0x3ff] {
                assert!(SavedFormat::duplicate(format, source.handle).is_err());
            }
        }

        #[test]
        fn bitmap_snapshot_owns_an_independent_gdi_object() {
            use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, GetBitmapBits};
            let pixel = [12u8, 34, 56, 78];
            let source = SavedFormat {
                format: BITMAP,
                handle: unsafe { CreateBitmap(1, 1, 1, 32, pixel.as_ptr().cast()) },
            };
            assert!(!source.handle.is_null());
            let copy = SavedFormat::duplicate(BITMAP, source.handle).unwrap();
            assert_ne!(source.handle, copy.handle);
            drop(source);
            let mut actual = [0u8; 4];
            assert_eq!(
                unsafe { GetBitmapBits(copy.handle, 4, actual.as_mut_ptr().cast()) },
                4
            );
            assert_eq!(actual, pixel);
        }
    }
}

#[cfg(windows)]
pub(in crate::background::replacement) use windows_clipboard::{
    paste, supports_paste, ClipboardTransaction,
};
