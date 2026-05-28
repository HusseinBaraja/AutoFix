mod background;
mod ipc;
mod settings;
mod storage;

use std::panic;

#[cfg(test)]
mod accessibility;
#[cfg(test)]
mod platform;
#[cfg(test)]
pub mod secrets;

pub fn run_background_entry() -> i32 {
    if let Err(error) = background::app_identity::set_current_process_app_identity() {
        eprintln!("failed to set app identity: {error}");
    }

    match background::run_background_mode() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn autofix_run_background() -> i32 {
    match panic::catch_unwind(run_background_entry) {
        Ok(code) => code,
        Err(_) => {
            eprintln!("panic in autofix_run_background");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_entrypoint_symbols_use_c_abi_return_codes() {
        let run: extern "C" fn() -> i32 = super::autofix_run_background;

        let _ = run;
    }
}
