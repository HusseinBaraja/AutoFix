mod accessibility;
mod ipc;
mod platform;
mod secrets;
mod settings;
mod storage;

use accessibility::ui_automation_root_available;
use ipc::named_pipe_round_trip_available;
use platform::active_window_title_len;
use secrets::dpapi_round_trip_available;
use settings::AppConfig;
use storage::open_memory_database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::parse("trigger = \"ctrl+space\"")?;
    let database = open_memory_database()?;
    let title_len = active_window_title_len();
    let ui_automation = ui_automation_root_available()?;
    let dpapi = dpapi_round_trip_available();
    let named_pipe = named_pipe_round_trip_available();

    println!(
        "AutoFix engine ready: trigger={}, sqlite={}, win32_title_len={}, ui_automation={}, dpapi={}, named_pipe={}",
        config.trigger,
        database.sqlite_version()?,
        title_len,
        ui_automation,
        dpapi,
        named_pipe
    );

    Ok(())
}
