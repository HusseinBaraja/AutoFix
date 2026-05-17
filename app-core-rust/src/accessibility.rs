use windows::core::Result;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

pub(crate) fn ui_automation_root_available() -> Result<bool> {
    unsafe {
        let initialization_result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let initialization_succeeded = initialization_result.is_ok();
        initialization_result.ok()?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_ui_automation_root() {
        assert!(ui_automation_root_available().unwrap());
    }
}
