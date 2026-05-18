mod accessibility;
mod ipc;
mod platform;
pub mod secrets;
mod settings;
mod storage;

use accessibility::ui_automation_root_available;
use ipc::named_pipe_round_trip_available;
use platform::active_window_title_len;
use secrets::dpapi_round_trip_available;
use settings::default_config_toml;
use storage::open_memory_database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_toml = default_config_toml()?;
    let database = open_memory_database()?;
    let title_len = active_window_title_len();
    let ui_automation = ui_automation_root_available()?;
    let dpapi = dpapi_round_trip_available();
    let named_pipe = named_pipe_round_trip_available();

    println!(
        "AutoFix engine ready: config_bytes={}, sqlite={}, win32_title_len={}, ui_automation={}, dpapi={}, named_pipe={}",
        config_toml.len(),
        database.sqlite_version()?,
        title_len,
        ui_automation,
        dpapi,
        named_pipe
    );

    Ok(())
}
