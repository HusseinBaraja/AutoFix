//! Process-scoped graceful stop signal shared with the Autofix.exe supervisor.
use std::ptr;
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_OBJECT_0},
    System::Threading::{CreateEventW, GetCurrentProcessId, WaitForSingleObject},
};

pub(in crate::background) struct ShutdownSignal(HANDLE);

impl ShutdownSignal {
    /// Create before normal processing, so the shell can stop even during startup.
    pub(in crate::background) fn new() -> Result<Self, u32> {
        let name: Vec<u16> = format!("Local\\AutoFix.EngineStop.{}", unsafe {
            GetCurrentProcessId()
        })
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let handle = unsafe { CreateEventW(ptr::null(), 1, 0, name.as_ptr()) };
        if handle.is_null() {
            Err(unsafe { GetLastError() })
        } else {
            Ok(Self(handle))
        }
    }

    /// Poll without blocking the Windows input/message loop.
    pub(in crate::background) fn requested(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for ShutdownSignal {
    /// Release the process-scoped event after engine teardown.
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An isolated manual-reset signal remains readable without blocking input processing.
    #[test]
    fn graceful_stop_signal_is_latched_and_polling_is_nonblocking() {
        // Keep the signal isolated from background-runtime tests in this process.
        let signal = ShutdownSignal(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) });
        assert!(!signal.0.is_null());
        assert!(!signal.requested());
        assert_ne!(
            unsafe { windows_sys::Win32::System::Threading::SetEvent(signal.0) },
            0
        );
        assert!(signal.requested());
        assert!(signal.requested());
    }
}
