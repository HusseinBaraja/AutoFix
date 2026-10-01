use std::{
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, GlobalFree, SetLastError, HANDLE, HWND},
    Graphics::Gdi::{CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteMetaFile, DeleteObject},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
            GetClipboardOwner, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
            SetClipboardData, METAFILEPICT,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
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
/// Compare independent format snapshots to detect a newer external clipboard copy.
fn same_published_data(format: u32, expected: HANDLE, actual: HANDLE) -> bool {
    if actual.is_null() {
        return false;
    }
    if expected == actual {
        return true;
    }
    if matches!(
        format,
        BITMAP | PALETTE | METAFILE | ENH_METAFILE | 0x82 | 0x83 | 0x8e
    ) {
        return native_object_bytes(format, expected)
            .zip(native_object_bytes(format, actual))
            .is_some_and(|(expected, actual)| expected == actual);
    }
    // GetClipboardData may return a different local HGLOBAL on the recovery thread.
    // Compare its bytes with our independent snapshot under the clipboard lock.
    unsafe {
        let length = GlobalSize(expected);
        if length == 0 || GlobalSize(actual) != length {
            return false;
        }
        let previous = GlobalLock(expected);
        let current = GlobalLock(actual);
        let same = !previous.is_null()
            && !current.is_null()
            && std::slice::from_raw_parts(previous.cast::<u8>(), length)
                == std::slice::from_raw_parts(current.cast::<u8>(), length);
        if !previous.is_null() {
            GlobalUnlock(expected);
        }
        if !current.is_null() {
            GlobalUnlock(actual);
        }
        same
    }
}

/// Fingerprint native graphics objects independently of thread-local clipboard handles.
fn native_object_bytes(format: u32, handle: HANDLE) -> Option<Vec<u8>> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetBitmapBits, GetEnhMetaFileBits, GetMetaFileBitsEx, GetObjectW, GetPaletteEntries,
        BITMAP as NativeBitmap, PALETTEENTRY,
    };
    /// Allocate native object bytes fallibly before accessing the copied object.
    fn buffer(length: usize) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).ok()?;
        bytes.resize(length, 0);
        Some(bytes)
    }
    if handle.is_null() {
        return None;
    }
    unsafe {
        match format {
            BITMAP | 0x82 => {
                let mut bitmap: NativeBitmap = std::mem::zeroed();
                if GetObjectW(
                    handle,
                    std::mem::size_of::<NativeBitmap>() as i32,
                    (&mut bitmap as *mut NativeBitmap).cast(),
                ) == 0
                {
                    return None;
                }
                let length = bitmap
                    .bmWidthBytes
                    .checked_mul(bitmap.bmHeight.checked_abs()?)?;
                let mut bits = buffer(usize::try_from(length).ok()?)?;
                if GetBitmapBits(handle, length, bits.as_mut_ptr().cast()) != length {
                    return None;
                }
                let mut bytes = [
                    bitmap.bmWidth,
                    bitmap.bmHeight,
                    bitmap.bmWidthBytes,
                    i32::from(bitmap.bmPlanes),
                    i32::from(bitmap.bmBitsPixel),
                ]
                .into_iter()
                .flat_map(i32::to_ne_bytes)
                .collect::<Vec<_>>();
                bytes.try_reserve(bits.len()).ok()?;
                bytes.extend(bits);
                Some(bytes)
            }
            PALETTE => {
                let count = GetPaletteEntries(handle, 0, 0, ptr::null_mut());
                if count == 0 {
                    return None;
                }
                let mut entries = Vec::<PALETTEENTRY>::new();
                entries.try_reserve_exact(count as usize).ok()?;
                entries.resize(count as usize, std::mem::zeroed());
                if GetPaletteEntries(handle, 0, count, entries.as_mut_ptr()) != count {
                    return None;
                }
                let mut bytes = buffer(
                    entries
                        .len()
                        .checked_mul(std::mem::size_of::<PALETTEENTRY>())?,
                )?;
                ptr::copy_nonoverlapping(
                    entries.as_ptr().cast::<u8>(),
                    bytes.as_mut_ptr(),
                    bytes.len(),
                );
                Some(bytes)
            }
            ENH_METAFILE | 0x8e => {
                let length = GetEnhMetaFileBits(handle, 0, ptr::null_mut());
                if length == 0 {
                    return None;
                }
                let mut bytes = buffer(length as usize)?;
                (GetEnhMetaFileBits(handle, length, bytes.as_mut_ptr()) == length).then_some(bytes)
            }
            METAFILE | 0x83 => {
                let picture = GlobalLock(handle).cast::<METAFILEPICT>();
                if picture.is_null() {
                    return None;
                }
                let dimensions = [(*picture).mm, (*picture).xExt, (*picture).yExt];
                let metafile = (*picture).hMF;
                GlobalUnlock(handle);
                let length = GetMetaFileBitsEx(metafile, 0, ptr::null_mut());
                if length == 0 {
                    return None;
                }
                let mut bits = buffer(length as usize)?;
                if GetMetaFileBitsEx(metafile, length, bits.as_mut_ptr().cast()) != length {
                    return None;
                }
                let mut bytes = dimensions
                    .into_iter()
                    .flat_map(i32::to_ne_bytes)
                    .collect::<Vec<_>>();
                bytes.try_reserve(bits.len()).ok()?;
                bytes.extend(bits);
                Some(bytes)
            }
            _ => None,
        }
    }
}

static RECOVERY_PENDING: AtomicBool = AtomicBool::new(false);
/// Create a message-only clipboard owner on the calling thread.
fn owner_window() -> HWND {
    unsafe {
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
    }
}

struct Recovery {
    saved: Vec<SavedFormat>,
    sequence: u32,
    owned: Vec<u32>,
    clipboard_owner: HWND,
    temporary_snapshot: Vec<SavedFormat>,
    restoring: bool,
}
// Exclusively owned duplicated HGLOBAL/GDI handles contain no Rust references.
// Windows permits their use and release from the recovery thread.
unsafe impl Send for Recovery {}

mod recovery;
use recovery::recovery_sender;
/// Drain and stop the recovery worker before the engine exits.
pub(super) fn shutdown() {
    recovery::shutdown();
}

/// Encode a NUL-terminated Windows string without borrowing temporary data.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

struct ClipboardLock;
impl ClipboardLock {
    /// Acquire the clipboard for a bounded period without modifying any formats.
    fn open(owner: HWND) -> Result<Self, String> {
        Self::open_with_messages(owner, false)
    }

    /// Recovery must answer sent owner messages while another thread holds the lock.
    fn open_with_messages(owner: HWND, pump: bool) -> Result<Self, String> {
        let deadline = Instant::now() + Duration::from_millis(250);
        loop {
            if pump {
                recovery::pump_messages();
            }
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
    /// Release owned native resources on scope exit without touching a newer copy.
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
    /// Copy readable formats into independent owned handles; refuse owner-managed data.
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

    /// Allocate movable clipboard memory and copy all bytes before publication.
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

    /// Transfer ownership to Windows only after SetClipboardData succeeds.
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
    /// Release owned native resources on scope exit without touching a newer copy.
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
    restore_copy: Option<Vec<SavedFormat>>,
    temporary: Vec<SavedFormat>,
    sequence: u32,
    owned: Vec<u32>,
    clipboard_owner: HWND,
    temporary_snapshot: Vec<SavedFormat>,
    restoring: bool,
    changed: bool,
    recovery_owner: bool,
}
pub(in crate::background::replacement) struct PreparationFailure {
    pub(in crate::background::replacement) reason: String,
    pub(in crate::background::replacement) clipboard_uncertain: bool,
}
impl From<String> for PreparationFailure {
    /// Convert a pre-write failure without claiming any clipboard mutation.
    fn from(reason: String) -> Self {
        Self {
            reason,
            clipboard_uncertain: false,
        }
    }
}
impl From<&str> for PreparationFailure {
    /// Convert a pre-write failure without claiming any clipboard mutation.
    fn from(reason: &str) -> Self {
        reason.to_owned().into()
    }
}
impl ClipboardTransaction {
    /// Snapshot every format and preallocate temporary and restoration data before any write.
    pub(in crate::background::replacement) fn prepare(text: &str) -> Result<Self, String> {
        if RECOVERY_PENDING.load(Ordering::Acquire) || super::super::shutting_down() {
            return Err("clipboard recovery is pending; use fallback".into());
        }
        // Prove recovery is available before any clipboard write.
        recovery_sender()?;
        let owner = owner_window();
        if owner.is_null() {
            return Err("clipboard owner window unavailable".into());
        }
        let mut transaction = Self {
            owner,
            saved: Vec::new(),
            restore_copy: None,
            temporary: Vec::new(),
            sequence: 0,
            owned: Vec::new(),
            clipboard_owner: ptr::null_mut(),
            temporary_snapshot: Vec::new(),
            restoring: false,
            changed: false,
            recovery_owner: false,
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
        // Reserve a complete restoration set before replacing the clipboard.
        transaction.restore_copy = Some(
            transaction
                .saved
                .iter()
                .map(|value| SavedFormat::duplicate(value.format, value.handle))
                .collect::<Result<Vec<_>, _>>()?,
        );
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
        transaction.temporary = temporary;
        transaction.temporary_snapshot = transaction
            .temporary
            .iter()
            .map(|value| SavedFormat::duplicate(value.format, value.handle))
            .collect::<Result<Vec<_>, _>>()?;
        transaction.owned =
            Vec::with_capacity(transaction.saved.len().max(transaction.temporary.len()));
        transaction.sequence = unsafe { GetClipboardSequenceNumber() };
        drop(lock);
        Ok(transaction)
    }

    /// Require an unchanged sequence and publish privacy markers before temporary text.
    pub(in crate::background::replacement) fn install(&mut self) -> Result<(), PreparationFailure> {
        if super::super::shutting_down() {
            return Err("clipboard replacement is shutting down".into());
        }
        let _lock = ClipboardLock::open(self.owner)?;
        if unsafe { GetClipboardSequenceNumber() } != self.sequence {
            return Err("clipboard changed before paste; newer clipboard retained".into());
        }
        if unsafe { EmptyClipboard() } == 0 {
            return Err("clipboard could not be prepared".into());
        }
        self.changed = true;
        self.owned.clear();
        self.clipboard_owner = self.owner;
        self.sequence = unsafe { GetClipboardSequenceNumber() };
        // Install privacy markers before text can be observed by history/cloud.
        for value in self.temporary.iter_mut().rev() {
            if let Err(error) = value.publish() {
                // We still hold the clipboard; restore directly before returning.
                let restored = self.restore_locked();
                self.sequence = unsafe { GetClipboardSequenceNumber() };
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
            self.owned.push(value.format);
        }
        self.sequence = unsafe { GetClipboardSequenceNumber() };
        Ok(())
    }

    /// Restore all originals while locked and retain snapshots on partial publication failure.
    fn restore_locked(&mut self) -> Result<(), String> {
        let owned = std::cell::RefCell::new(std::mem::take(&mut self.owned));
        let cleared = std::cell::Cell::new(false);
        let restored = super::restore_snapshot(
            &self.saved,
            &mut self.restore_copy,
            |value| SavedFormat::duplicate(value.format, value.handle),
            || {
                if unsafe { EmptyClipboard() } != 0 {
                    owned.borrow_mut().clear();
                    cleared.set(true);
                    Ok(())
                } else {
                    Err("clipboard restore failed".into())
                }
            },
            |value| {
                value.publish()?;
                owned.borrow_mut().push(value.format);
                Ok(())
            },
        );
        self.owned = owned.into_inner();
        if cleared.get() {
            self.clipboard_owner = self.owner;
            self.restoring = true;
        }
        self.sequence = unsafe { GetClipboardSequenceNumber() };
        if restored.is_ok() {
            self.changed = false;
        }
        restored
    }

    /// Verify owner, independent published bytes and formats before overwriting anything.
    fn owns_current_locked(&self) -> bool {
        if unsafe { GetClipboardOwner() } != self.clipboard_owner {
            return false;
        }
        // Closing/reading the clipboard can synthesize formats and change its sequence.
        let expected = if self.restoring {
            &self.saved
        } else {
            &self.temporary_snapshot
        };
        if self.owned.iter().any(|format| {
            expected
                .iter()
                .find(|value| value.format == *format)
                .is_none_or(|value| {
                    !same_published_data(*format, value.handle, unsafe {
                        GetClipboardData(*format)
                    })
                })
        }) {
            return false;
        }
        let mut format = 0;
        loop {
            unsafe {
                SetLastError(0);
            }
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                return unsafe { GetLastError() } == 0;
            }
            if !super::owned_format_or_synthesis(&self.owned, format) {
                return false;
            }
        }
    }

    /// Restore only our own clipboard state; preserve a newer external copy.
    pub(in crate::background::replacement) fn restore(&mut self) -> Result<(), String> {
        if !self.changed {
            return Ok(());
        }
        let _lock = ClipboardLock::open_with_messages(self.owner, self.recovery_owner)?;
        if !self.owns_current_locked() {
            self.changed = false;
            return Err("clipboard changed externally; newer clipboard retained".into());
        }
        self.restore_locked()
    }
}
impl Drop for ClipboardTransaction {
    /// Release owned native resources on scope exit without touching a newer copy.
    fn drop(&mut self) {
        if self.recovery_owner {
            return;
        }
        if let Err(reason) = self.restore() {
            tracing::warn!(
                reason,
                recovery_pending = self.changed,
                "clipboard restoration requires recovery"
            );
            if self.changed {
                RECOVERY_PENDING.store(true, Ordering::Release);
                // Destroying the owner retains already-rendered data with no owner.
                // Recovery checks independent copies of published data under the lock.
                unsafe {
                    DestroyWindow(self.owner);
                }
                let recovery = Recovery {
                    saved: std::mem::take(&mut self.saved),
                    sequence: self.sequence,
                    owned: std::mem::take(&mut self.owned),
                    clipboard_owner: ptr::null_mut(),
                    temporary_snapshot: std::mem::take(&mut self.temporary_snapshot),
                    restoring: self.restoring,
                };
                // The worker starts before mutation and is drained on graceful exit.
                if recovery_sender()
                    .and_then(|sender| {
                        sender
                            .send(Some(recovery))
                            .map_err(|_| "clipboard recovery worker stopped".into())
                    })
                    .is_err()
                {
                    tracing::warn!("clipboard recovery worker stopped; clipboard method disabled");
                }
                return;
            }
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
    let length = unsafe { GetClassNameW(window as HWND, class.as_mut_ptr(), class.len() as i32) };
    let class = String::from_utf16_lossy(&class[..length.max(0) as usize]).to_ascii_lowercase();
    matches!(
        class.as_str(),
        "edit" | "richedit20w" | "richedit50w" | "richedit20a" | "richedit"
    )
}

/// Use bounded synchronous WM_PASTE; timeout means target mutation is uncertain.
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
mod tests;
