mod background;
mod ipc;
mod settings;
mod storage;

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

pub fn shutdown_all_entry() -> i32 {
    if let Err(error) = background::app_identity::set_current_process_app_identity() {
        eprintln!("failed to set app identity: {error}");
    }

    background::shutdown_process_group_mode();
    0
}

#[no_mangle]
pub extern "C" fn autofix_run_background() -> i32 {
    run_background_entry()
}

#[no_mangle]
pub extern "C" fn autofix_shutdown_all() -> i32 {
    shutdown_all_entry()
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_entrypoint_symbols_use_c_abi_return_codes() {
        let run: extern "C" fn() -> i32 = super::autofix_run_background;
        let shutdown: extern "C" fn() -> i32 = super::autofix_shutdown_all;

        let _ = run;
        let _ = shutdown;
    }
}
