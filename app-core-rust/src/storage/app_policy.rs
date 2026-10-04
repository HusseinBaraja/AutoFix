//! Serialize outbound and replacement authorization with app-policy writes.

use std::{path::Path, time::Duration};

use rusqlite::{Connection, OpenFlags, Result};

use super::{repositories::AppRuleRepository, AppRule};

pub(crate) struct AppPolicyGuard {
    connection: Connection,
}

impl AppPolicyGuard {
    /// Admission may read committed policy during an API send. Never create,
    /// migrate, reserve a writer, or wait for a contended policy database.
    pub(crate) fn read_rules_nowait(path: &Path) -> Result<Vec<AppRule>> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::ZERO)?;
        reject_pending_import(&connection)?;
        AppRuleRepository::new(&connection).list()
    }

    /// Reserve the writer slot before reading, even in WAL mode. Never create or migrate.
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        // Contention denies the operation immediately; never wait on policy writers.
        connection.busy_timeout(Duration::ZERO)?;
        connection.execute_batch("BEGIN IMMEDIATE")?;
        reject_pending_import(&connection)?;
        Ok(Self { connection })
    }

    /// Read rules while the same transaction prevents a concurrent revocation commit.
    pub(crate) fn rules(&self) -> Result<Vec<AppRule>> {
        AppRuleRepository::new(&self.connection).list()
    }
}

/// A surviving import decision prevents outbound requests and replacement until settings reconciliation.
fn reject_pending_import(connection: &Connection) -> Result<()> {
    if crate::settings::import_recovery::pending(connection)? {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
}

impl Drop for AppPolicyGuard {
    /// Closing the connection rolls back the read-only transaction and releases writers.
    fn drop(&mut self) {
        if let Err(error) = self.connection.execute_batch("ROLLBACK") {
            tracing::warn!(%error, "failed to release app policy transaction");
        }
    }
}
