pub(crate) const APP_USER_MODEL_ID: &str = "Zerone.Autofix";

pub(crate) fn set_current_process_app_identity() -> Result<(), String> {
    native::set_current_process_app_identity(APP_USER_MODEL_ID)
}

#[cfg(windows)]
mod native {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    pub(super) fn set_current_process_app_identity(app_id: &str) -> Result<(), String> {
        let app_id = std::ffi::OsStr::new(app_id)
            .encode_wide()
            .chain(once(0))
            .collect::<Vec<_>>();
        let result = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
        if result >= 0 {
            Ok(())
        } else {
            Err(format!(
                "SetCurrentProcessExplicitAppUserModelID failed with HRESULT 0x{result:08X}"
            ))
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub(super) fn set_current_process_app_identity(_app_id: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_user_model_id_is_shared_shell_identity() {
        assert_eq!(APP_USER_MODEL_ID, "Zerone.Autofix");
    }
}
