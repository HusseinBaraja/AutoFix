mod accessibility;
mod platform;
mod settings;
mod storage;

use accessibility::ui_automation_root_available;
use platform::active_window_title_len;
use settings::AppConfig;
use storage::open_memory_database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::parse("trigger = \"ctrl+space\"")?;
    let database = open_memory_database()?;
    let title_len = active_window_title_len();
    let ui_automation = ui_automation_root_available()?;

    println!(
        "AutoFix engine ready: trigger={}, sqlite={}, win32_title_len={}, ui_automation={}",
        config.trigger,
        database.sqlite_version()?,
        title_len,
        ui_automation
    );

    Ok(())
}
