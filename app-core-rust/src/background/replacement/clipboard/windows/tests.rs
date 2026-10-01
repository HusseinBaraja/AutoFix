use super::*;

/// Inspect native objects without printing any user clipboard bytes.
unsafe fn fingerprint(format: u32, handle: HANDLE) -> Vec<u8> {
    use windows_sys::Win32::{
        Graphics::Gdi::{
            GetBitmapBits, GetEnhMetaFileBits, GetMetaFileBitsEx, GetObjectW, GetPaletteEntries,
            BITMAP as NativeBitmap, PALETTEENTRY,
        },
        System::Memory::GlobalSize,
    };
    match format {
        BITMAP | 0x82 => {
            let mut bitmap: NativeBitmap = std::mem::zeroed();
            assert!(
                GetObjectW(
                    handle,
                    std::mem::size_of::<NativeBitmap>() as i32,
                    (&mut bitmap as *mut NativeBitmap).cast()
                ) != 0
            );
            let length = (bitmap.bmWidthBytes * bitmap.bmHeight.abs()) as usize;
            let mut bytes = vec![0; length];
            assert!(
                GetBitmapBits(handle, length as i32, bytes.as_mut_ptr().cast()) as usize == length
            );
            bytes
        }
        PALETTE => {
            let count = GetPaletteEntries(handle, 0, 0, ptr::null_mut());
            let mut entries: Vec<PALETTEENTRY> = vec![std::mem::zeroed(); count as usize];
            assert!(GetPaletteEntries(handle, 0, count, entries.as_mut_ptr()) == count);
            std::slice::from_raw_parts(
                entries.as_ptr().cast::<u8>(),
                entries.len() * std::mem::size_of::<PALETTEENTRY>(),
            )
            .to_vec()
        }
        ENH_METAFILE | 0x8e => {
            let length = GetEnhMetaFileBits(handle, 0, ptr::null_mut());
            let mut bytes = vec![0; length as usize];
            assert!(GetEnhMetaFileBits(handle, length, bytes.as_mut_ptr()) == length);
            bytes
        }
        METAFILE | 0x83 => {
            let picture = GlobalLock(handle).cast::<METAFILEPICT>();
            assert!(!picture.is_null());
            let mut bytes = [(*picture).mm, (*picture).xExt, (*picture).yExt]
                .into_iter()
                .flat_map(i32::to_ne_bytes)
                .collect::<Vec<_>>();
            let metafile = (*picture).hMF;
            GlobalUnlock(handle);
            let length = GetMetaFileBitsEx(metafile, 0, ptr::null_mut());
            let mut bits = vec![0; length as usize];
            assert!(GetMetaFileBitsEx(metafile, length, bits.as_mut_ptr().cast()) == length);
            bytes.extend(bits);
            bytes
        }
        _ => {
            let length = GlobalSize(handle);
            let memory = GlobalLock(handle);
            assert!(!memory.is_null());
            let bytes = std::slice::from_raw_parts(memory.cast::<u8>(), length).to_vec();
            GlobalUnlock(handle);
            bytes
        }
    }
}

/// Verify native preservation, bounded quarantine, newer copies, and both shutdown outcomes.
#[test]
#[ignore = "runs native clipboard operations in a child with an isolated window station"]
fn native_clipboard_preservation_smoke() {
    use std::{os::windows::process::CommandExt, process::Command};
    use windows_sys::Win32::{
        Graphics::Gdi::CreateBitmap, System::StationsAndDesktops::*,
        UI::WindowsAndMessaging::WINSTA_ALL_ACCESS,
    };
    if std::env::var("AUTOFIX_CLIPBOARD_TEST_HOST").as_deref() != Ok("1") {
        for exhausted_exit in ["0", "1"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "native_clipboard_preservation_smoke",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("AUTOFIX_CLIPBOARD_TEST_HOST", "1")
                .env("AUTOFIX_CLIPBOARD_TEST_EXHAUSTED_EXIT", exhausted_exit)
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated clipboard test failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    // A clipboard belongs to a window station. Never write to the user's station.
    let station = unsafe {
        CreateWindowStationW(
            ptr::null(),
            0, // System-named noninteractive station; no administrator rights required.
            WINSTA_ALL_ACCESS as u32,
            ptr::null(),
        )
    };
    assert!(
        !station.is_null(),
        "isolated station unavailable: Windows error {}",
        unsafe { GetLastError() }
    );
    let mut flags: USEROBJECTFLAGS = unsafe { std::mem::zeroed() };
    assert!(
        unsafe {
            GetUserObjectInformationW(
                station,
                UOI_FLAGS,
                (&mut flags as *mut USEROBJECTFLAGS).cast(),
                std::mem::size_of::<USEROBJECTFLAGS>() as u32,
                ptr::null_mut(),
            )
        } != 0
    );
    assert!(
        flags.dwFlags & 1 == 0,
        "refuse the visible user window station"
    );
    assert!(unsafe { SetProcessWindowStation(station) } != 0);
    let desktop = unsafe {
        CreateDesktopW(
            wide("test").as_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            0x01ff,
            ptr::null(),
        )
    };
    assert!(!desktop.is_null(), "isolated desktop unavailable");
    assert!(unsafe { SetThreadDesktop(desktop) } != 0);
    let seed_owner = owner_window();
    assert!(!seed_owner.is_null());
    let seed_lock = ClipboardLock::open(seed_owner).unwrap();
    if unsafe { EnumClipboardFormats(0) } != 0 {
        // Clean only the exact seed left by an earlier run of this isolated test.
        let marker = unsafe { RegisterClipboardFormatW(wide("AutoFix test binary").as_ptr()) };
        let marker_handle = unsafe { GetClipboardData(marker) };
        let prior_text = wide(if marker_handle.is_null() {
            "pending recovery"
        } else {
            "isolated original rich text"
        })
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
        assert!(
            unsafe { fingerprint(UNICODE_TEXT, GetClipboardData(UNICODE_TEXT)) }
                .starts_with(&prior_text)
        );
        if !marker_handle.is_null() {
            assert!(unsafe { fingerprint(marker, marker_handle) }.starts_with(&[0, 255, 1, 0]));
        } else {
            let privacy =
                unsafe { RegisterClipboardFormatW(wide("CanIncludeInClipboardHistory").as_ptr()) };
            let value = unsafe { GetClipboardData(privacy) };
            assert!(!value.is_null());
            assert!(unsafe { fingerprint(privacy, value) }.starts_with(&[0, 0, 0, 0]));
        }
        assert!(unsafe { EmptyClipboard() } != 0);
    }
    assert!(
        unsafe { EnumClipboardFormats(0) } == 0,
        "test station clipboard must be empty"
    );
    assert!(unsafe { EmptyClipboard() } != 0);
    let text = wide("isolated original rich text")
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    let mut seeds = vec![SavedFormat::bytes(UNICODE_TEXT, &text).unwrap()];
    for (name, bytes) in [
        ("Rich Text Format", b"{\\rtf1\\b original}\0".as_slice()),
        ("HTML Format", b"<b>original</b>\0".as_slice()),
        ("AutoFix test binary", &[0u8, 255, 1, 0][..]),
    ] {
        let format = unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) };
        seeds.push(SavedFormat::bytes(format, bytes).unwrap());
    }
    let pixel = [12u8, 34, 56, 78];
    seeds.push(SavedFormat {
        format: BITMAP,
        handle: unsafe { CreateBitmap(1, 1, 1, 32, pixel.as_ptr().cast()) },
    });
    for seed in &mut seeds {
        seed.publish().unwrap();
    }
    drop(seed_lock);
    let mut transaction = match ClipboardTransaction::prepare("the") {
        Ok(transaction) => transaction,
        Err(reason) => {
            panic!("clipboard preservation preflight refused safely: {reason}")
        }
    };
    // Failures while fingerprinting occur before the first clipboard write.
    let expected: Vec<_> = transaction
        .saved
        .iter()
        .map(|value| {
            (value.format, unsafe {
                fingerprint(value.format, value.handle)
            })
        })
        .collect();
    transaction
        .install()
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    let edit = unsafe {
        CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("teh AFTER").as_ptr(),
            windows_sys::Win32::UI::WindowsAndMessaging::WS_POPUP,
            0,
            0,
            200,
            80,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        )
    };
    assert!(!edit.is_null());
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(edit, 0x00b1, 0, 3);
    }
    super::super::paste_and_restore(|| paste(edit as isize), || transaction.restore()).unwrap();
    let mut text = [0u16; 32];
    let length = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(
            edit,
            text.as_mut_ptr(),
            text.len() as i32,
        )
    };
    assert_eq!(
        String::from_utf16_lossy(&text[..length as usize]),
        "the AFTER"
    );
    unsafe {
        DestroyWindow(edit);
    }
    let _lock = ClipboardLock::open(transaction.owner).unwrap();
    if expected.is_empty() {
        assert!(unsafe { EnumClipboardFormats(0) } == 0);
    }
    let expected_formats: std::collections::BTreeSet<_> =
        expected.iter().map(|(format, _)| *format).collect();
    let mut restored_formats = std::collections::BTreeSet::new();
    let mut format = 0;
    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        restored_formats.insert(format);
    }
    assert_eq!(
        restored_formats, expected_formats,
        "temporary clipboard formats survived restoration"
    );
    for (format, bytes) in &expected {
        let restored = unsafe { GetClipboardData(*format) };
        assert!(!restored.is_null(), "clipboard format {format} missing");
        assert!(
            unsafe { fingerprint(*format, restored) } == *bytes,
            "clipboard format {format} changed"
        );
    }
    drop(_lock);
    drop(transaction);
    // A locked clipboard must retain recovery data until it can be restored.
    let mut blocked = ClipboardTransaction::prepare("pending recovery").unwrap();
    blocked
        .install()
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    let (ready, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let holder = thread::spawn(move || {
        let lock = ClipboardLock::open(ptr::null_mut()).unwrap();
        ready.send(()).unwrap();
        released.recv().unwrap();
        drop(lock);
    });
    started.recv().unwrap();
    assert!(blocked.restore().is_err());
    drop(blocked);
    assert!(RECOVERY_PENDING.load(Ordering::Acquire));
    // Keep the lock beyond the active retry budget. Originals and cross-app
    // exclusion must survive exhaustion until controlled recovery can resume.
    thread::sleep(Duration::from_millis(2500));
    assert!(RECOVERY_PENDING.load(Ordering::Acquire));
    assert!(ClipboardTransaction::prepare("another app correction").is_err());
    release.send(()).unwrap();
    holder.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while RECOVERY_PENDING.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !RECOVERY_PENDING.load(Ordering::Acquire),
        "recovery did not finish"
    );
    let lock = ClipboardLock::open(seed_owner).unwrap();
    for (format, bytes) in &expected {
        assert!(
            unsafe { fingerprint(*format, GetClipboardData(*format)) } == *bytes,
            "recovered clipboard format {format} changed"
        );
    }
    assert!(unsafe { EmptyClipboard() } != 0);
    drop(lock);
    let mut superseded = ClipboardTransaction::prepare("superseded correction").unwrap();
    superseded
        .install()
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    let lock = ClipboardLock::open(seed_owner).unwrap();
    assert!(unsafe { EmptyClipboard() } != 0);
    let newer = wide("new copy")
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    SavedFormat::bytes(UNICODE_TEXT, &newer)
        .unwrap()
        .publish()
        .unwrap();
    drop(lock);
    assert!(superseded.restore().is_err());
    let lock = ClipboardLock::open(seed_owner).unwrap();
    let actual = unsafe { fingerprint(UNICODE_TEXT, GetClipboardData(UNICODE_TEXT)) };
    assert!(actual.starts_with(&newer), "newer copy was overwritten");
    assert!(unsafe { EmptyClipboard() } != 0); // Return the test station to its original empty state.
    drop(lock);

    // A permanently invalid publication must terminate, scrub temporary text,
    // and release ownership rather than republish forever.
    let lock = ClipboardLock::open(seed_owner).unwrap();
    let original = wide("saved original")
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    SavedFormat::bytes(UNICODE_TEXT, &original)
        .unwrap()
        .publish()
        .unwrap();
    drop(lock);
    let mut permanent = ClipboardTransaction::prepare("temporary failure text").unwrap();
    permanent
        .install()
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    permanent
        .saved
        .push(SavedFormat::bytes(0, b"invalid format").unwrap());
    permanent.restore_copy = None;
    drop(permanent);
    thread::sleep(Duration::from_millis(2500));
    assert!(
        RECOVERY_PENDING.load(Ordering::Acquire),
        "permanent failure released exclusion"
    );
    assert!(ClipboardTransaction::prepare("another app correction").is_err());
    let lock = ClipboardLock::open(seed_owner).unwrap();
    assert!(
        unsafe { fingerprint(UNICODE_TEXT, GetClipboardData(UNICODE_TEXT)) }.starts_with(&original)
    );
    drop(lock);
    let sequence = unsafe { GetClipboardSequenceNumber() };
    thread::sleep(Duration::from_millis(500));
    assert_eq!(
        unsafe { GetClipboardSequenceNumber() },
        sequence,
        "quarantine kept republishing partial data"
    );
    let lock = ClipboardLock::open(seed_owner).unwrap();
    assert!(unsafe { EmptyClipboard() } != 0);
    SavedFormat::bytes(UNICODE_TEXT, &original)
        .unwrap()
        .publish()
        .unwrap();
    drop(lock);
    let deadline = Instant::now() + Duration::from_secs(2);
    while RECOVERY_PENDING.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !RECOVERY_PENDING.load(Ordering::Acquire),
        "new copy did not end quarantine"
    );

    // EmptyClipboard sends WM_DESTROYCLIPBOARD to the owner synchronously.
    // The recovery thread must service it even when the caller holds the lock.
    let mut copied = ClipboardTransaction::prepare("temporary copy text").unwrap();
    copied
        .install()
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    copied.recovery_owner = true;
    let (ready, started) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let copier = thread::spawn(move || {
        let owner = owner_window();
        let lock = ClipboardLock::open(owner).unwrap();
        ready.send(()).unwrap();
        assert!(unsafe { EmptyClipboard() } != 0);
        let text = wide("concurrent new copy")
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect::<Vec<_>>();
        SavedFormat::bytes(UNICODE_TEXT, &text)
            .unwrap()
            .publish()
            .unwrap();
        drop(lock);
        unsafe { DestroyWindow(owner) };
        done.send(()).unwrap();
    });
    started.recv_timeout(Duration::from_secs(1)).unwrap();
    recovery::recover(&mut copied, Duration::from_millis(500));
    unsafe { DestroyWindow(copied.owner) };
    finished
        .recv_timeout(Duration::from_secs(1))
        .expect("Copy blocked on clipboard owner");
    copier.join().unwrap();
    assert!(!copied.changed, "newer copy must end recovery");
    drop(copied);

    // Graceful shutdown waits for queued restoration after a lock is released.
    let mut exiting = ClipboardTransaction::prepare("temporary exit text").unwrap();
    exiting
        .install()
        .unwrap_or_else(|failure| panic!("{}", failure.reason));
    if std::env::var("AUTOFIX_CLIPBOARD_TEST_EXHAUSTED_EXIT").as_deref() == Ok("1") {
        exiting
            .saved
            .push(SavedFormat::bytes(0, b"invalid format").unwrap());
        exiting.restore_copy = None;
        drop(exiting);
        assert!(RECOVERY_PENDING.load(Ordering::Acquire));
        assert!(!shutdown(), "incomplete cleanup was reported as clean");
        assert!(RECOVERY_PENDING.load(Ordering::Acquire));
        assert!(ClipboardTransaction::prepare("another app correction").is_err());
        let lock = ClipboardLock::open(seed_owner).unwrap();
        assert!(unsafe { EmptyClipboard() } != 0);
        drop(lock);
        return;
    }
    let (ready, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let holder = thread::spawn(move || {
        let lock = ClipboardLock::open(ptr::null_mut()).unwrap();
        ready.send(()).unwrap();
        released.recv().unwrap();
        thread::sleep(Duration::from_millis(100));
        drop(lock);
    });
    started.recv().unwrap();
    drop(exiting);
    assert!(RECOVERY_PENDING.load(Ordering::Acquire));
    release.send(()).unwrap();
    assert!(shutdown(), "restored clipboard was reported as incomplete");
    holder.join().unwrap();
    assert!(!RECOVERY_PENDING.load(Ordering::Acquire));
    assert!(
        recovery_sender().is_err(),
        "shutdown must not restart recovery"
    );
    let lock = ClipboardLock::open(seed_owner).unwrap();
    let bytes = unsafe { fingerprint(UNICODE_TEXT, GetClipboardData(UNICODE_TEXT)) };
    let expected = wide("concurrent new copy")
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    assert!(
        bytes.starts_with(&expected),
        "shutdown lost the saved clipboard"
    );
    assert!(unsafe { EmptyClipboard() } != 0);
    drop(lock);
}

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
    assert!(same_published_data(BITMAP, source.handle, copy.handle));
    drop(source);
    let mut actual = [0u8; 4];
    assert_eq!(
        unsafe { GetBitmapBits(copy.handle, 4, actual.as_mut_ptr().cast()) },
        4
    );
    assert_eq!(actual, pixel);
}
