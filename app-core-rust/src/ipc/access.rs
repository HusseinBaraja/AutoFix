//! Local IPC is private to the pipe owner; remote clients remain rejected by the server.

use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, PSECURITY_DESCRIPTOR,
        SECURITY_ATTRIBUTES,
    },
};

/// Own the descriptor until Windows has copied it during pipe creation.
pub(super) struct PipeSecurity(PSECURITY_DESCRIPTOR);

impl PipeSecurity {
    /// Grant only the object's owner access; never inherit Everyone or anonymous read permissions.
    pub(super) fn new() -> std::io::Result<Self> {
        let sddl: Vec<u16> = "D:P(A;;GA;;;OW)\0".encode_utf16().collect();
        let mut descriptor = null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Self(descriptor))
        }
    }

    /// Pipe handles are not inheritable; the protected DACL applies to every new instance.
    pub(super) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
