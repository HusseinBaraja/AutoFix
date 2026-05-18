mod logs;
mod migrations;
mod repositories;
#[cfg(test)]
mod tests;
mod types;

use std::path::Path;

use rusqlite::{Connection, Result};

pub(crate) use logs::{CorrectionMetadataRepository, DebugEventRepository};
pub(crate) use repositories::{
    AppRuleRepository, CustomDictionaryRepository, LanguageOverrideRepository,
    LearnedRuleRepository,
};
pub(crate) use types::{
    AppRule, CorrectionMetadata, CustomDictionaryEntry, DebugEvent, LanguageOverride,
    LearnedCorrectionRule,
};

pub(crate) struct Database {
    connection: Connection,
}

impl Database {
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
            .query_row("select version from schema_migrations", [], |row| {
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

pub(crate) fn open_memory_database() -> Result<Database> {
    Database::open_memory()
}
