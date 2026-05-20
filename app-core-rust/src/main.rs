#[cfg(test)]
mod accessibility;
mod background;
#[cfg(test)]
mod ipc;
mod platform;
#[cfg(test)]
pub mod secrets;
mod settings;
mod storage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    background::run_background_mode().map_err(Into::into)
}
