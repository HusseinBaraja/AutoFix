//! Hold one engine lease per Windows session, including standalone native entrypoints.
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::CreateMutexW,
};

pub(in crate::background) struct EngineLease(HANDLE);

impl EngineLease {
    pub(in crate::background) fn acquire() -> Result<Self, u32> {
        Self::acquire_named("Local\\AutoFix.BackgroundEngine")
    }

    fn acquire_named(name: &str) -> Result<Self, u32> {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // Object existence is the lease. Do not acquire recursive thread ownership.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        let error = unsafe { GetLastError() };
        if handle.is_null() {
            Err(error)
        } else if error == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            Err(error)
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for EngineLease {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_engine_is_refused_and_exit_releases_lease() {
        let name = format!("Local\\AutoFix.EngineLeaseTest.{}", std::process::id());
        let first = EngineLease::acquire_named(&name).unwrap();
        assert_eq!(
            EngineLease::acquire_named(&name).err(),
            Some(ERROR_ALREADY_EXISTS)
        );
        // Refusing another opener must not release the original engine's lease.
        assert_eq!(
            EngineLease::acquire_named(&name).err(),
            Some(ERROR_ALREADY_EXISTS)
        );
        drop(first);
        assert!(EngineLease::acquire_named(&name).is_ok());
    }
}
