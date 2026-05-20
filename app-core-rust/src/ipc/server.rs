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
    Storage::FileSystem::{ReadFile, WriteFile, PIPE_ACCESS_DUPLEX},
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
            tracing::warn!("failed to wake IPC server through request path: {}", error);
            wake_connect_named_pipe(&self.pipe_path);
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

fn create_pipe(pipe_path: &str) -> Result<HANDLE, String> {
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
            null_mut(),
        )
    };

    if pipe == INVALID_HANDLE_VALUE {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(pipe)
    }
}

fn handle_pipe(pipe: HANDLE, state: &Arc<Mutex<IpcServerState>>) {
    let connected = unsafe { ConnectNamedPipe(pipe, null_mut()) };
    if connected == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32)
    {
        return;
    }

    let response = read_request(pipe)
        .and_then(|request| {
            state
                .lock()
                .map_err(|_| "IPC state lock poisoned".to_owned())
                .map(|mut state| state.handle(request))
        })
        .unwrap_or_else(IpcResponse::error);

    let _ = write_response(pipe, &response);
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

    if write_ok == 0 || written != output.len() as u32 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn pipe_path_for_process(name: &str) -> String {
    format!(r"\\.\pipe\Local\{name}")
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}
