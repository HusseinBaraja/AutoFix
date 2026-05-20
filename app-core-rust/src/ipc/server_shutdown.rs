use std::{
    ffi::OsStr,
    iter::once,
    os::windows::ffi::OsStrExt,
    ptr::null_mut,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
};

const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn join_worker(worker: JoinHandle<()>) {
    let deadline = Instant::now() + WORKER_SHUTDOWN_TIMEOUT;
    while !worker.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    if !worker.is_finished() {
        tracing::error!(
            "IPC server worker did not shut down within {:?}",
            WORKER_SHUTDOWN_TIMEOUT
        );
        return;
    }

    if worker.join().is_err() {
        tracing::error!("IPC server worker panicked during shutdown");
    }
}

pub(super) fn wake_connect_named_pipe(pipe_path: &str) {
    let path = wide(pipe_path);
    let pipe = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };

    if pipe != INVALID_HANDLE_VALUE {
        unsafe {
            CloseHandle(pipe);
        }
    } else {
        tracing::warn!(
            "fallback IPC wake could not connect to pipe: {}",
            std::io::Error::last_os_error()
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}
