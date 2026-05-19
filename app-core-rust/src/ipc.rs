use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW, PIPE_READMODE_MESSAGE,
    PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

fn named_pipe_round_trip(name: &str) -> Result<Vec<u8>, &'static str> {
    let pipe_path = format!(r"\\.\pipe\{name}-{}", std::process::id());
    let server_path = pipe_path.clone();
    let (ready_tx, ready_rx) = mpsc::channel();

    let server = thread::spawn(move || serve_one_message(&server_path, ready_tx));
    ready_rx.recv().map_err(|_| "pipe server did not start")??;
    let client_result = call_pipe(&pipe_path, b"ping");
    let server_result = server.join().map_err(|_| "pipe server panicked")?;

    server_result?;
    client_result
}

fn serve_one_message(
    pipe_path: &str,
    ready: mpsc::Sender<Result<(), &'static str>>,
) -> Result<(), &'static str> {
    let path = wide(pipe_path);
    let pipe = unsafe {
        CreateNamedPipeW(
            path.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            512,
            512,
            0,
            null_mut(),
        )
    };

    if pipe == INVALID_HANDLE_VALUE {
        let _ = ready.send(Err("CreateNamedPipeW failed"));
        return Err("CreateNamedPipeW failed");
    }

    let _ = ready.send(Ok(()));
    let result = serve_connected_pipe(pipe);
    unsafe {
        DisconnectNamedPipe(pipe);
        CloseHandle(pipe);
    }

    result
}

fn serve_connected_pipe(pipe: HANDLE) -> Result<(), &'static str> {
    let connected = unsafe { ConnectNamedPipe(pipe, null_mut()) };
    if connected == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32)
    {
        return Err("ConnectNamedPipe failed");
    }

    let mut buffer = [0_u8; 512];
    let mut read = 0;
    let read_ok = unsafe {
        ReadFile(
            pipe,
            buffer.as_mut_ptr() as *mut _,
            buffer.len() as u32,
            &mut read,
            null_mut(),
        )
    };

    if read_ok == 0 || &buffer[..read as usize] != b"ping" {
        return Err("ReadFile from pipe failed");
    }

    let mut written = 0;
    let write_ok = unsafe {
        WriteFile(
            pipe,
            b"pong".as_ptr() as *const _,
            4,
            &mut written,
            null_mut(),
        )
    };

    if write_ok == 0 || written != 4 {
        return Err("WriteFile to pipe failed");
    }

    Ok(())
}

fn call_pipe(pipe_path: &str, message: &[u8]) -> Result<Vec<u8>, &'static str> {
    let path = wide(pipe_path);
    let mut pipe = INVALID_HANDLE_VALUE;

    for _ in 0..20 {
        pipe = unsafe {
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
            break;
        }

        unsafe {
            WaitNamedPipeW(path.as_ptr(), 50);
        }

        thread::sleep(Duration::from_millis(10));
    }

    if pipe == INVALID_HANDLE_VALUE {
        return Err("CreateFileW for pipe failed");
    }

    let result = write_then_read(pipe, message);
    unsafe {
        CloseHandle(pipe);
    }

    result
}

fn write_then_read(pipe: HANDLE, message: &[u8]) -> Result<Vec<u8>, &'static str> {
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
        return Err("WriteFile client failed");
    }

    let mut buffer = [0_u8; 512];
    let mut read = 0;
    let read_ok = unsafe {
        ReadFile(
            pipe,
            buffer.as_mut_ptr() as *mut _,
            buffer.len() as u32,
            &mut read,
            null_mut(),
        )
    };

    if read_ok == 0 {
        return Err("ReadFile client failed");
    }

    Ok(buffer[..read as usize].to_vec())
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchanges_message_over_named_pipe() -> Result<(), &'static str> {
        let reply = named_pipe_round_trip("autofix-pipe-test")?;

        assert_eq!(reply, b"pong");
        Ok(())
    }
}
