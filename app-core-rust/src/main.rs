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
    let supported_args = std::env::args().skip(1).all(|arg| arg == "--background");
    if !supported_args {
        eprintln!("usage: background-engine.exe [--background]");
        std::process::exit(2);
    }

    background::run_background_mode().map_err(Into::into)
}
