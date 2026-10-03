use std::{
    ffi::OsStr,
    iter::once,
    os::windows::ffi::OsStrExt,
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_MORE_DATA, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{FlushFileBuffers, ReadFile, WriteFile, PIPE_ACCESS_DUPLEX},
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT,
    },
};

use super::{
    protocol::IpcRequest,
    server_shutdown::{join_worker, wake_connect_named_pipe},
    IpcResponse, IpcServerState,
};

pub(crate) const PIPE_NAME: &str = "AutoFix.Background.Ipc";
const INITIAL_READ_BUFFER_SIZE: usize = 64 * 1024;

pub(crate) struct NamedPipeIpcServer {
    pipe_path: String,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl NamedPipeIpcServer {
    pub(crate) fn start(state: IpcServerState) -> Self {
        Self::start_for_path(pipe_path_for_process(PIPE_NAME), state)
    }

    pub(crate) fn start_for_path(pipe_path: String, state: IpcServerState) -> Self {
        let state = Arc::new(Mutex::new(state));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker = {
            let state = Arc::clone(&state);
            let shutdown = Arc::clone(&shutdown);
            let worker_pipe_path = pipe_path.clone();
            thread::spawn(move || serve_pipe(worker_pipe_path, state, shutdown))
        };

        Self {
            pipe_path,
            shutdown,
            worker: Some(worker),
        }
    }

    pub(crate) fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Err(error) =
            super::client::send_request(&self.pipe_path, &IpcRequest::IsBackgroundRunning)
        {
            tracing::debug!("IPC server request wake skipped: {}", error);
            if let Err(error) = wake_connect_named_pipe(&self.pipe_path) {
                tracing::debug!("IPC server fallback wake skipped: {}", error);
            }
        }
        if let Some(worker) = self.worker.take() {
            join_worker(worker);
        }
    }
}

fn serve_pipe(pipe_path: String, state: Arc<Mutex<IpcServerState>>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        match create_pipe(&pipe_path) {
            Ok(pipe) => {
                handle_pipe(pipe, &state);
                unsafe {
                    DisconnectNamedPipe(pipe);
                    CloseHandle(pipe);
                }
            }
            Err(error) => {
                tracing::error!("failed to create IPC pipe: {}", error);
                break;
            }
        }
    }
}

/// Create a local, owner-only endpoint; security setup failure never falls back to a default ACL.
fn create_pipe(pipe_path: &str) -> Result<HANDLE, String> {
    let security = super::access::PipeSecurity::new().map_err(|error| error.to_string())?;
    let attributes = security.attributes();
    let path = wide(pipe_path);
    let pipe = unsafe {
        CreateNamedPipeW(
            path.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64 * 1024,
            64 * 1024,
            0,
            &attributes,
        )
    };

    if pipe == INVALID_HANDLE_VALUE {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(pipe)
    }
}

/// Bound request/response pipe I/O separately from application work, then drain before disconnect.
fn handle_pipe(pipe: HANDLE, state: &Arc<Mutex<IpcServerState>>) {
    handle_pipe_with(pipe, |request| {
        state
            .lock()
            .map_err(|_| "IPC state lock poisoned".to_owned())
            .map(|mut state| state.handle(request))
            .unwrap_or_else(IpcResponse::error)
    });
}

fn handle_pipe_with(pipe: HANDLE, handle_request: impl FnOnce(IpcRequest) -> IpcResponse) {
    let connected = unsafe { ConnectNamedPipe(pipe, null_mut()) };
    if connected == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32)
    {
        return;
    }

    let request = {
        let Ok(deadline) = super::request_deadline::RequestDeadline::start(pipe) else {
            tracing::warn!("cannot enforce IPC request read deadline");
            return;
        };
        let request = read_request(pipe);
        if deadline.expired() {
            return;
        }
        request
    };
    // CancelIoEx cannot cancel a state lock or SQLite work. Do not let that work
    // expire a later response before any response I/O has even started.
    let response = request
        .map(handle_request)
        .unwrap_or_else(IpcResponse::error);
    let Ok(_deadline) = super::request_deadline::RequestDeadline::start(pipe) else {
        tracing::warn!("cannot enforce IPC response delivery deadline");
        return;
    };
    if let Err(error) = write_response(pipe, &response) {
        tracing::warn!(%error, "IPC response delivery failed");
    }
}

fn read_request(pipe: HANDLE) -> Result<IpcRequest, String> {
    let mut buffer = vec![0_u8; INITIAL_READ_BUFFER_SIZE];
    let mut total_read = 0_usize;

    loop {
        if total_read == buffer.len() {
            buffer.resize(buffer.len() * 2, 0);
        }

        let capacity = buffer.len() - total_read;
        let mut read = 0;
        let read_ok = unsafe {
            ReadFile(
                pipe,
                buffer[total_read..].as_mut_ptr() as *mut _,
                capacity as u32,
                &mut read,
                null_mut(),
            )
        };

        if read_ok == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_MORE_DATA as i32) {
                return Err(error.to_string());
            }
        }

        total_read += read as usize;
        if read as usize == 0 || read as usize != capacity {
            break;
        }
    }

    serde_json::from_slice(&buffer[..total_read]).map_err(|error| error.to_string())
}

/// Preserve the complete response for delayed readers and clients that wait for EOF.
fn write_response(pipe: HANDLE, response: &IpcResponse) -> Result<(), String> {
    let output = serde_json::to_vec(response).map_err(|error| error.to_string())?;
    let mut written = 0;
    let write_ok = unsafe {
        WriteFile(
            pipe,
            output.as_ptr() as *const _,
            output.len() as u32,
            &mut written,
            null_mut(),
        )
    };

    if write_ok == 0 || written != output.len() as u32 || unsafe { FlushFileBuffers(pipe) } == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        // DisconnectNamedPipe discards unread bytes. Drain before the caller disconnects,
        // including clients that consume a byte stream until EOF instead of message boundaries.
        Ok(())
    }
}

pub(crate) fn pipe_path_for_process(name: &str) -> String {
    format!(r"\\.\pipe\Local\{name}")
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn slow_request_handling_does_not_consume_pipe_io_deadlines() {
        let pipe_path = pipe_path_for_process(&format!(
            "AutoFix.SlowHandler.{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let server_path = pipe_path.clone();
        let (ready, started) = mpsc::channel();
        let worker = thread::spawn(move || {
            let pipe = create_pipe(&server_path).unwrap();
            ready.send(()).unwrap();
            handle_pipe_with(pipe, |request| {
                assert!(matches!(request, IpcRequest::IsBackgroundRunning));
                // Deterministic stand-in for storage work or a contended state lock.
                thread::sleep(Duration::from_millis(1200));
                IpcResponse::BackgroundRunning(super::super::protocol::BackgroundRunningResponse {
                    running: true,
                })
            });
            unsafe {
                DisconnectNamedPipe(pipe);
                CloseHandle(pipe);
            }
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let response =
            super::super::client::send_request(&pipe_path, &IpcRequest::IsBackgroundRunning);
        worker.join().unwrap();
        assert!(matches!(response, Ok(IpcResponse::BackgroundRunning(status)) if status.running));
    }
}
