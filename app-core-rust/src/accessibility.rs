use windows::core::Result;
use windows::Win32::Foundation::{S_FALSE, S_OK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

pub(crate) fn ui_automation_root_available() -> Result<bool> {
    unsafe {
        let initialization_result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let initialization_succeeded =
            initialization_result == S_OK || initialization_result == S_FALSE;
        accept_com_initialization(initialization_result)?;

        let result = (|| {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            automation.GetRootElement()?;
            Ok(true)
        })();

        if initialization_succeeded {
            CoUninitialize();
        }
        result
    }
}

fn accept_com_initialization(initialization_result: windows::core::HRESULT) -> Result<()> {
    if initialization_result == S_OK || initialization_result == S_FALSE {
        Ok(())
    } else {
        initialization_result.ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::HRESULT;

    #[test]
    fn opens_ui_automation_root() {
        assert!(ui_automation_root_available().unwrap());
    }

    #[test]
    fn accepts_com_already_initialized_result() {
        assert!(accept_com_initialization(S_FALSE).is_ok());
    }

    #[test]
    fn rejects_failed_com_initialization_result() {
        assert!(accept_com_initialization(HRESULT(0x80004005_u32 as i32)).is_err());
    }
}
