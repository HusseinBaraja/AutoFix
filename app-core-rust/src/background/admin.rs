use crate::background::BackgroundError;

pub(crate) fn reject_elevated_process() -> Result<(), BackgroundError> {
    reject_elevated_state(process_is_elevated())
}

fn reject_elevated_state(is_elevated: bool) -> Result<(), BackgroundError> {
    if is_elevated {
        Err(BackgroundError::ElevatedProcess)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn process_is_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned_size = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned_size,
        );
        CloseHandle(token);

        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
fn process_is_elevated() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_normal_user_process() {
        assert!(reject_elevated_state(false).is_ok());
    }

    #[test]
    fn rejects_elevated_process() {
        assert!(matches!(
            reject_elevated_state(true),
            Err(BackgroundError::ElevatedProcess)
        ));
    }
}
