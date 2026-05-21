use std::{
    error::Error, ffi::OsStr, fmt, iter::once, os::windows::ffi::OsStrExt, ptr::null_mut, thread,
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA,
        ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
    System::Pipes::{SetNamedPipeHandleState, WaitNamedPipeW, PIPE_READMODE_MESSAGE},
};

use super::protocol::{IpcRequest, IpcResponse};

const READ_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) enum IpcClientError {
    Unavailable,
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for IpcClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(formatter, "background process is unavailable"),
            Self::Io(source) => write!(formatter, "IPC I/O error: {}", source),
            Self::Json(source) => write!(formatter, "IPC JSON error: {}", source),
        }
    }
}

impl Error for IpcClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            Self::Json(source) => Some(source),
            Self::Unavailable => None,
        }
    }
}

pub(crate) fn send_request(
    pipe_path: &str,
    request: &IpcRequest,
) -> Result<IpcResponse, IpcClientError> {
    let mut last_error = None;
    for _ in 0..20 {
        let pipe = connect(pipe_path)?;
        let result = write_then_read(pipe, request);
        unsafe {
            CloseHandle(pipe);
        }

        match result {
            Ok(response) => return Ok(response),
            Err(IpcClientError::Io(error)) if is_transient_disconnect(&error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }

    Err(IpcClientError::Io(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::TimedOut, "IPC request timed out")
    })))
}

fn connect(pipe_path: &str) -> Result<HANDLE, IpcClientError> {
    let path = wide(pipe_path);

    for _ in 0..20 {
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
            set_message_read_mode(pipe)?;
            return Ok(pipe);
        }

        let error = std::io::Error::last_os_error();
        match error.raw_os_error().map(|code| code as u32) {
            Some(ERROR_PIPE_BUSY) => unsafe {
                WaitNamedPipeW(path.as_ptr(), 50);
            },
            Some(ERROR_FILE_NOT_FOUND) => thread::sleep(Duration::from_millis(10)),
            _ => thread::sleep(Duration::from_millis(10)),
        }
    }

    Err(IpcClientError::Unavailable)
}

fn set_message_read_mode(pipe: HANDLE) -> Result<(), IpcClientError> {
    let mut mode = PIPE_READMODE_MESSAGE;
    let mode_ok = unsafe { SetNamedPipeHandleState(pipe, &mut mode, null_mut(), null_mut()) };
    if mode_ok != 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    unsafe {
        CloseHandle(pipe);
    }
    Err(IpcClientError::Io(error))
}

fn write_then_read(pipe: HANDLE, request: &IpcRequest) -> Result<IpcResponse, IpcClientError> {
    let message = serde_json::to_vec(request).map_err(IpcClientError::Json)?;
    let mut written = 0;
    let write_ok = unsafe {
        WriteFile(
            pipe,
            message.as_ptr() as *const _,
            message.len() as u32,
            &mut written,
            null_mut(),
        )
    };

    if write_ok == 0 || written != message.len() as u32 {
        return Err(IpcClientError::Io(std::io::Error::last_os_error()));
    }

    let response = read_response_bytes(pipe)?;

    serde_json::from_slice(&response).map_err(IpcClientError::Json)
}

fn read_response_bytes(pipe: HANDLE) -> Result<Vec<u8>, IpcClientError> {
    let mut response = Vec::new();
    let mut chunk = vec![0_u8; READ_CHUNK_SIZE];

    loop {
        let mut read = 0;
        let read_ok = unsafe {
            ReadFile(
                pipe,
                chunk.as_mut_ptr() as *mut _,
                chunk.len() as u32,
                &mut read,
                null_mut(),
            )
        };
        response.extend_from_slice(&chunk[..read as usize]);

        if read_ok != 0 {
            continue;
        }

        let error = unsafe { GetLastError() };
        if matches!(error, ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED) && !response.is_empty() {
            return Ok(response);
        }

        if error != ERROR_MORE_DATA {
            return Err(IpcClientError::Io(std::io::Error::from_raw_os_error(
                error as i32,
            )));
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

fn is_transient_disconnect(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_PIPE_NOT_CONNECTED as i32)
}
