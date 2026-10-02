//! Bound a connected client's pipe I/O, including waiting for response consumption.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use windows_sys::Win32::{Foundation::HANDLE, System::IO::CancelIoEx};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct RequestDeadline {
    expired: Arc<AtomicBool>,
    done: mpsc::Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl RequestDeadline {
    /// The caller must keep the pipe alive until this guard is dropped and its watchdog joined.
    pub(super) fn start(pipe: HANDLE) -> std::io::Result<Self> {
        let expired = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&expired);
        let (done, complete) = mpsc::channel();
        let pipe_address = pipe as usize;
        let worker = thread::Builder::new()
            .name("autofix-ipc-deadline".into())
            .spawn(move || {
                if complete.recv_timeout(REQUEST_TIMEOUT) != Err(mpsc::RecvTimeoutError::Timeout) {
                    return;
                }
                flag.store(true, Ordering::Release);
                loop {
                    // Retry covers a deadline that raced between two pipe I/O calls.
                    // Cancellation targets this pipe only, never SQLite or settings-file I/O.
                    unsafe {
                        CancelIoEx(pipe_address as HANDLE, std::ptr::null());
                    }
                    if complete.recv_timeout(Duration::from_millis(10))
                        != Err(mpsc::RecvTimeoutError::Timeout)
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            expired,
            done,
            worker: Some(worker),
        })
    }

    /// Refuse new work after expiry, even if no pipe operation was pending when cancellation began.
    pub(super) fn expired(&self) -> bool {
        self.expired.load(Ordering::Acquire)
    }
}

impl Drop for RequestDeadline {
    fn drop(&mut self) {
        let _ = self.done.send(());
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            tracing::error!("IPC deadline worker panicked");
        }
    }
}
