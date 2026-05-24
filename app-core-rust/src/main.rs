#[cfg(test)]
mod accessibility;
mod background;
mod ipc;
mod platform;
#[cfg(test)]
pub mod secrets;
mod settings;
mod storage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let supported_args = args.is_empty()
        || args.as_slice() == ["--background"]
        || args.as_slice() == ["--shutdown-all"];
    if !supported_args {
        eprintln!("usage: background-engine.exe [--background|--shutdown-all]");
        std::process::exit(2);
    }

    if args.as_slice() == ["--shutdown-all"] {
        background::shutdown_process_group_mode();
        return Ok(());
    }

    background::run_background_mode().map_err(Into::into)
}
