mod platform;
mod settings;
mod storage;

use platform::active_window_title_len;
use settings::AppConfig;
use storage::open_memory_database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::parse("trigger = \"ctrl+space\"")?;
    let database = open_memory_database()?;
    let title_len = active_window_title_len();

    println!(
        "AutoFix engine ready: trigger={}, sqlite={}, win32_title_len={}",
        config.trigger,
        database.sqlite_version()?,
        title_len
    );

    Ok(())
}
