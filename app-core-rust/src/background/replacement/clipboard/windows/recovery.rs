//! Bounded, message-responsive recovery, joined before graceful engine exit.
use super::*;
use std::sync::Mutex;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

const RECOVERY_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_ATTEMPTS: usize = 8;

struct Worker {
    sender: mpsc::Sender<Option<Recovery>>,
    thread: thread::JoinHandle<()>,
}

#[derive(Default)]
struct State {
    worker: Option<Worker>,
    stopped: bool,
}

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
                return;
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
                recover(&mut transaction, RECOVERY_TIMEOUT);
                drop(transaction);
                // Rendered data survives destruction, but no later copy waits on
                // an idle receiver to process WM_DESTROYCLIPBOARD.
                unsafe { DestroyWindow(owner) };
                RECOVERY_PENDING.store(false, Ordering::Release);
                owner = owner_window();
                if owner.is_null() {
                    tracing::warn!("clipboard recovery window unavailable");
                    break;
                }
            }
            unsafe { DestroyWindow(owner) };
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
pub(super) fn shutdown() {
    let worker = {
        let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
        state.stopped = true;
        state.worker.take()
    };
    if let Some(worker) = worker {
        let _ = worker.sender.send(None);
        if worker.thread.join().is_err() {
            tracing::warn!("clipboard recovery worker panicked during shutdown");
        }
    }
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
pub(super) fn recover(transaction: &mut ClipboardTransaction, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    for attempt in 0..MAX_ATTEMPTS {
        pump_messages();
        if !transaction.changed || Instant::now() >= deadline {
            break;
        }
        if let Err(reason) = transaction.restore() {
            tracing::debug!(reason, "clipboard recovery attempt deferred");
        }
        if !transaction.changed {
            break;
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
            "clipboard recovery exhausted; clipboard cleanup incomplete"
        );
    }
}
