//! Bounded, message-responsive recovery, joined before graceful engine exit.
use super::*;
use std::sync::Mutex;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

const RECOVERY_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_ATTEMPTS: usize = 8;
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecoveryOutcome {
    Restored,
    Superseded,
    Exhausted,
}

struct Worker {
    sender: mpsc::Sender<Option<Recovery>>,
    thread: thread::JoinHandle<bool>,
}

#[derive(Default)]
struct State {
    worker: Option<Worker>,
    stopped: bool,
}

/// Keep the worker lifecycle separate from the exclusion gate for unresolved data.
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

/// Start recovery before mutation; a stopped worker cannot be restarted on exit.
pub(super) fn recovery_sender() -> Result<mpsc::Sender<Option<Recovery>>, String> {
    let mut state = state()
        .lock()
        .map_err(|_| "clipboard recovery state unavailable")?;
    if state.stopped {
        return Err("clipboard recovery is shutting down".into());
    }
    if let Some(worker) = &state.worker {
        if worker.thread.is_finished() {
            return Err("clipboard recovery worker stopped".into());
        }
        return Ok(worker.sender.clone());
    }
    if crate::background::replacement::shutting_down() {
        return Err("clipboard recovery is shutting down".into());
    }
    let (sender, receiver) = mpsc::channel::<Option<Recovery>>();
    let (ready, started) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("clipboard-recovery".into())
        .spawn(move || {
            let mut owner = owner_window();
            let _ = ready.send(!owner.is_null());
            if owner.is_null() {
                return false;
            }
            while let Ok(Some(recovery)) = receiver.recv() {
                let mut transaction = ClipboardTransaction {
                    owner,
                    saved: recovery.saved,
                    restore_copy: None,
                    temporary: Vec::new(),
                    sequence: recovery.sequence,
                    owned: recovery.owned,
                    clipboard_owner: recovery.clipboard_owner,
                    temporary_snapshot: recovery.temporary_snapshot,
                    restoring: recovery.restoring,
                    changed: true,
                    recovery_owner: true,
                };
                let mut outcome = recover(&mut transaction, RECOVERY_TIMEOUT);
                if outcome == RecoveryOutcome::Exhausted {
                    // Retain originals and the global gate, but never leave a
                    // clipboard-owning window blocked on an idle receiver.
                    release_owner(&mut transaction);
                    outcome = quarantine(&mut transaction);
                }
                owner = transaction.owner;
                drop(transaction);
                // Rendered data survives destruction, but no later copy waits on
                // an idle receiver to process WM_DESTROYCLIPBOARD.
                unsafe { DestroyWindow(owner) };
                if !finish_recovery(outcome) {
                    return false;
                }
                owner = owner_window();
                if owner.is_null() {
                    tracing::warn!("clipboard recovery window unavailable");
                    return false;
                }
            }
            unsafe { DestroyWindow(owner) };
            !RECOVERY_PENDING.load(Ordering::Acquire)
        })
        .map_err(|_| "clipboard recovery worker unavailable")?;
    if !started.recv().unwrap_or(false) {
        let _ = thread.join();
        return Err("clipboard recovery window unavailable".into());
    }
    state.worker = Some(Worker {
        sender: sender.clone(),
        thread,
    });
    Ok(sender)
}

/// Drain queued recovery within its normal retry budget, then release its window.
pub(super) fn shutdown() -> bool {
    STOP_REQUESTED.store(true, Ordering::Release);
    let worker = {
        let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
        state.stopped = true;
        state.worker.take()
    };
    if let Some(worker) = worker {
        let _ = worker.sender.send(None);
        match worker.thread.join() {
            Ok(clean) => return clean,
            Err(_) => tracing::warn!("clipboard recovery worker panicked during shutdown"),
        }
    }
    !RECOVERY_PENDING.load(Ordering::Acquire)
}

/// Deliver sent clipboard-owner messages even while OpenClipboard is busy.
pub(super) fn pump_messages() {
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// Bound permanent publication failures and back off instead of churning Copy.
pub(super) fn recover(
    transaction: &mut ClipboardTransaction,
    timeout: Duration,
) -> RecoveryOutcome {
    let deadline = Instant::now() + timeout;
    for attempt in 0..MAX_ATTEMPTS {
        pump_messages();
        if !transaction.changed || Instant::now() >= deadline {
            break;
        }
        match transaction.restore() {
            Ok(()) => return RecoveryOutcome::Restored,
            Err(_) if !transaction.changed => return RecoveryOutcome::Superseded,
            Err(reason) => tracing::debug!(reason, "clipboard recovery attempt deferred"),
        }
        let delay = Duration::from_millis((25u64 << attempt).min(250));
        let next = (Instant::now() + delay).min(deadline);
        while Instant::now() < next {
            pump_messages();
            thread::sleep(Duration::from_millis(5));
        }
    }
    if transaction.changed {
        tracing::warn!(
            method = "clipboard",
            attempts_limit = MAX_ATTEMPTS,
            "clipboard recovery exhausted; originals retained and clipboard quarantined"
        );
        RecoveryOutcome::Exhausted
    } else {
        RecoveryOutcome::Restored
    }
}

/// Release clipboard ownership while keeping a fresh window available for controlled cleanup.
fn release_owner(transaction: &mut ClipboardTransaction) {
    if transaction.clipboard_owner == transaction.owner {
        transaction.clipboard_owner = ptr::null_mut();
    }
    unsafe { DestroyWindow(transaction.owner) };
    transaction.owner = owner_window();
}

/// Probe without republishing partial originals; resume one restore when a busy lock clears.
/// A newer copy ends quarantine. Shutdown gets one final cleanup attempt.
fn quarantine(transaction: &mut ClipboardTransaction) -> RecoveryOutcome {
    let mut resume_restore = !transaction.restoring;
    while !STOP_REQUESTED.load(Ordering::Acquire) {
        if let Ok(_lock) = ClipboardLock::open_with_messages(transaction.owner, true) {
            if !transaction.owns_current_locked() {
                transaction.changed = false;
                return RecoveryOutcome::Superseded;
            }
            if resume_restore && !transaction.owner.is_null() {
                resume_restore = false;
                if transaction.restore_locked().is_ok() {
                    return RecoveryOutcome::Restored;
                }
                // A partial restore must not be emptied and republished on each probe.
                drop(_lock);
                release_owner(transaction);
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
    if transaction.owner.is_null() {
        return RecoveryOutcome::Exhausted;
    }
    match transaction.restore() {
        Ok(()) => RecoveryOutcome::Restored,
        Err(_) if !transaction.changed => RecoveryOutcome::Superseded,
        Err(_) => RecoveryOutcome::Exhausted,
    }
}

/// Only proven restoration or supersession releases clipboard exclusion across apps.
pub(super) fn finish_recovery(outcome: RecoveryOutcome) -> bool {
    if outcome == RecoveryOutcome::Exhausted {
        tracing::warn!(
            method = "clipboard",
            "engine stopped with incomplete clipboard cleanup"
        );
        false
    } else {
        RECOVERY_PENDING.store(false, Ordering::Release);
        true
    }
}
