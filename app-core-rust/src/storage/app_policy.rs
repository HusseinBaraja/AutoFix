//! Serialize outbound authorization with SQLite policy writes from IPC and the UI.

use std::{path::Path, time::Duration};

use rusqlite::{Connection, OpenFlags, Result};

use super::{repositories::AppRuleRepository, AppRule};

pub(crate) struct AppPolicyGuard {
    connection: Connection,
}

impl AppPolicyGuard {
    /// Reserve the writer slot before reading, even in WAL mode. Never create or migrate.
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        // Contention denies this send immediately; do not extend its API deadline.
        connection.busy_timeout(Duration::ZERO)?;
        connection.execute_batch("BEGIN IMMEDIATE")?;
        Ok(Self { connection })
    }

    /// Read rules while the same transaction prevents a concurrent revocation commit.
    pub(crate) fn rules(&self) -> Result<Vec<AppRule>> {
        AppRuleRepository::new(&self.connection).list()
    }
}

impl Drop for AppPolicyGuard {
    /// Closing the connection rolls back the read-only transaction and releases writers.
    fn drop(&mut self) {
        if let Err(error) = self.connection.execute_batch("ROLLBACK") {
            tracing::warn!(%error, "failed to release outbound policy transaction");
        }
    }
}
