#![allow(dead_code)]

mod app_policy;
mod logs;
mod migrations;
mod repositories;
#[cfg(test)]
mod tests;
mod types;

use std::path::Path;

use rusqlite::{Connection, OpenFlags, Result};

pub(crate) use app_policy::AppPolicyGuard;
use logs::{CorrectionMetadataRepository, DebugEventRepository};
use repositories::{
    AppRuleRepository, CustomDictionaryRepository, LanguageOverrideRepository,
    LearnedRuleRepository,
};
pub(crate) use types::AppRule;
pub(crate) use types::CorrectionMetadata;
#[cfg(test)]
use types::{CustomDictionaryEntry, LanguageOverride, LearnedCorrectionRule};

pub(crate) struct Database {
    connection: Connection,
}

impl Database {
    /// Record optional telemetry without migrations, file creation, or waiting for a writer.
    /// Policy reservations take priority; busy or unavailable storage loses only metadata.
    pub(crate) fn record_metadata_nowait(path: &Path, metadata: &CorrectionMetadata) -> Result<()> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(std::time::Duration::ZERO)?;
        CorrectionMetadataRepository::new(&connection).record(metadata)
    }

    /// Access persistent exclusions independently of whether new learning is enabled.
    pub(crate) fn dictionary(&self) -> crate::dictionary::Repository<'_> {
        crate::dictionary::Repository::new(&self.connection)
    }
    /// File identity for fresh policy reads on a separate transport connection.
    pub(crate) fn path(&self) -> Option<&Path> {
        self.connection
            .path()
            .filter(|path| !path.is_empty())
            .map(Path::new)
    }

    pub(crate) fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        migrations::migrate(&connection)?;

        Ok(Self { connection })
    }

    pub(crate) fn open_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        migrations::migrate(&connection)?;

        Ok(Self { connection })
    }

    pub(crate) fn sqlite_version(&self) -> Result<String> {
        self.connection
            .query_row("select sqlite_version()", [], |row| row.get(0))
    }

    pub(crate) fn schema_version(&self) -> Result<i64> {
        self.connection
            .query_row("select max(version) from schema_migrations", [], |row| {
                row.get(0)
            })
    }

    pub(crate) fn app_rules(&self) -> AppRuleRepository<'_> {
        AppRuleRepository::new(&self.connection)
    }

    pub(crate) fn custom_dictionary(&self) -> CustomDictionaryRepository<'_> {
        CustomDictionaryRepository::new(&self.connection)
    }

    pub(crate) fn learned_rules(&self) -> LearnedRuleRepository<'_> {
        LearnedRuleRepository::new(&self.connection)
    }

    pub(crate) fn language_overrides(&self) -> LanguageOverrideRepository<'_> {
        LanguageOverrideRepository::new(&self.connection)
    }

    pub(crate) fn correction_metadata(&self) -> CorrectionMetadataRepository<'_> {
        CorrectionMetadataRepository::new(&self.connection)
    }

    pub(crate) fn debug_events(&self) -> DebugEventRepository<'_> {
        DebugEventRepository::new(&self.connection)
    }

    pub(crate) fn clear_logs(&self) -> Result<()> {
        self.connection.execute("delete from debug_events", [])?;
        self.connection
            .execute("delete from correction_metadata", [])?;
        Ok(())
    }
}
