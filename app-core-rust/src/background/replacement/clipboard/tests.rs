use super::*;
use std::cell::{Cell, RefCell};

#[test]
fn synthesized_formats_are_allowed_but_unrelated_changes_are_not() {
    for format in [1, 7, 13, 16] {
        assert!(owned_format_or_synthesis(&[13, 0xc001], format));
    }
    assert!(owned_format_or_synthesis(&[13, 0xc001], 0xc001));
    for format in [2, 3, 14, 0xc002] {
        assert!(!owned_format_or_synthesis(&[13, 0xc001], format));
    }
    assert!(!owned_format_or_synthesis(&[], 13));
}

#[test]
fn paste_requires_restore_and_restores_before_verification() {
    for pasted in [true, false] {
        for restored in [true, false] {
            let calls = RefCell::new(Vec::new());
            let result = paste_and_restore(
                || {
                    calls.borrow_mut().push("paste");
                    if pasted {
                        Ok(())
                    } else {
                        Err("paste failed".into())
                    }
                },
                || {
                    calls.borrow_mut().push("restore");
                    if restored {
                        Ok(())
                    } else {
                        Err("restore failed".into())
                    }
                },
            );
            if result.is_ok() {
                calls.borrow_mut().push("verify");
            }
            assert_eq!(result.is_ok(), pasted && restored);
            assert_eq!(&calls.borrow()[..2], &["paste", "restore"]);
            if !restored {
                assert!(result.unwrap_err().contains("restore failed"));
            }
        }
    }
}

#[test]
fn restore_is_allocation_free_and_preserves_all_formats_and_empty_clipboard() {
    for saved in [
        Vec::new(),
        vec![
            b"plain".to_vec(),
            b"{\\rtf1 rich}".to_vec(),
            vec![0, 255, 1],
        ],
    ] {
        let clipboard = RefCell::new(vec![b"correction".to_vec()]);
        let mut reserve = Some(saved.clone());
        restore_snapshot(
            &saved,
            &mut reserve,
            |_| panic!("initial restore must use its preallocated copies"),
            || {
                clipboard.borrow_mut().clear();
                Ok(())
            },
            |value| {
                clipboard.borrow_mut().push(std::mem::take(value));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*clipboard.borrow(), saved);
    }
}

#[test]
fn partial_restore_is_failure_scrubs_correction_and_retains_originals_for_retry() {
    let saved = vec![b"plain".to_vec(), b"rich".to_vec(), vec![0, 255, 1]];
    let originals = saved.clone();
    let clipboard = RefCell::new(vec![b"correction".to_vec()]);
    let mut reserve = Some(saved.clone());
    let attempted = Cell::new(0);
    let result = restore_snapshot(
        &saved,
        &mut reserve,
        |value| Ok(value.clone()),
        || {
            clipboard.borrow_mut().clear();
            Ok(())
        },
        |value| {
            attempted.set(attempted.get() + 1);
            if value == b"rich" {
                return Err("rich format failed".into());
            }
            clipboard.borrow_mut().push(std::mem::take(value));
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(attempted.get(), 3);
    assert_eq!(saved, originals);
    assert!(!clipboard.borrow().contains(&b"correction".to_vec()));
    restore_snapshot(
        &saved,
        &mut reserve,
        |value| Ok(value.clone()),
        || {
            clipboard.borrow_mut().clear();
            Ok(())
        },
        |value| {
            clipboard.borrow_mut().push(std::mem::take(value));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(*clipboard.borrow(), saved);
}

#[test]
fn busy_restore_keeps_reserved_data_and_failed_recovery_copy_scrubs_temporary_text() {
    let saved = vec![b"rich".to_vec()];
    let mut reserve = Some(saved.clone());
    let result = restore_snapshot(
        &saved,
        &mut reserve,
        |_| panic!("reserved"),
        || Err("busy".into()),
        |_| panic!("must not publish while busy"),
    );
    assert!(result.is_err());
    assert_eq!(reserve, Some(saved.clone()));
    reserve = None;
    let cleared = Cell::new(false);
    let result = restore_snapshot(
        &saved,
        &mut reserve,
        |_| Err("copy failed".into()),
        || {
            cleared.set(true);
            Ok(())
        },
        |_| panic!("failed copy"),
    );
    assert!(result.is_err());
    assert!(cleared.get());
    assert_eq!(saved, vec![b"rich".to_vec()]);
}

#[test]
fn restore_failure_prefers_fallback_only_for_affected_app() {
    let mut policy = ClipboardPolicy::default();
    assert!(policy.allowed("Editor.exe"));
    policy.failed("Editor.exe");
    assert!(!policy.allowed("EDITOR.EXE"));
    assert!(policy.allowed("other.exe"));
}
