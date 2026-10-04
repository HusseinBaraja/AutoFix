//! Shared Windows settings lock and SQLite-backed interrupted-import reconciliation.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};
#[cfg(not(windows))]
use std::fs;

/// Opens the same non-shared lock file as WPF. A process exit releases ownership automatically.
pub(super) fn acquire(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(suffixed(path, ".lock"))
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

/// Flushes content and atomically replaces the Windows file before the journal can be cleared.
pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = suffixed(path, ".native-save.tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Both names are nul-terminated; neither pointer outlives its backing vector.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(&temporary, path)?;
        File::open(path.parent().unwrap_or(Path::new(".")))?.sync_all()?;
    }
    Ok(())
}

/// Must run with the settings lock held. Failed recovery retains the durable decision and denies policy use.
pub(super) fn recover(path: &Path) -> io::Result<bool> {
    let database = path
        .parent()
        .unwrap_or(Path::new("."))
        .join("autofix.sqlite");
    if !database.exists() {
        return Ok(false);
    }
    let mut connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(io::Error::other)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(io::Error::other)?;
    if !pending(&connection).map_err(io::Error::other)? {
        return Ok(false);
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(io::Error::other)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(io::Error::other)?;
    let bytes: Option<Vec<u8>> = transaction
        .query_row(
            "select case committed when 0 then original_settings when 1 then imported_settings end
         from settings_import_recovery where id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(io::Error::other)?;
    let Some(bytes) = bytes else {
        return Ok(false);
    };
    write_atomic(path, &bytes)?;
    transaction
        .execute("delete from settings_import_recovery where id = 1", [])
        .map_err(io::Error::other)?;
    transaction.commit().map_err(io::Error::other)?;
    Ok(true)
}

/// Read-only admission check; missing legacy tables are allowed, unreadable recovery state is denied.
pub(crate) fn pending(connection: &Connection) -> rusqlite::Result<bool> {
    let table: bool = connection.query_row(
        "select exists(select 1 from sqlite_master where type = 'table' and name = 'settings_import_recovery')",
        [], |row| row.get(0),
    )?;
    if !table {
        return Ok(false);
    }
    connection.query_row(
        "select exists(select 1 from settings_import_recovery)",
        [],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests;
