use rusqlite::{params, Connection, Result};

use super::{
    migrations,
    types::{AppRule, CustomDictionaryEntry, LanguageOverride, LearnedCorrectionRule},
};

pub(crate) struct AppRuleRepository<'a> {
    connection: &'a Connection,
}

impl<'a> AppRuleRepository<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn upsert(&self, rule: &AppRule) -> Result<()> {
        self.connection.execute(
            "
            insert into app_rules (
                process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                api_engine_allowed
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            on conflict(process_name, window_title_pattern) do update set
                list_behavior = excluded.list_behavior,
                manual_shortcut_allowed = excluded.manual_shortcut_allowed,
                word_count_trigger_allowed = excluded.word_count_trigger_allowed,
                character_trigger_allowed = excluded.character_trigger_allowed,
                local_engine_allowed = excluded.local_engine_allowed,
                api_engine_allowed = excluded.api_engine_allowed,
                updated_at = current_timestamp
            ",
            params![
                rule.process_name,
                rule.window_title_pattern.as_deref().unwrap_or(""),
                rule.list_behavior,
                rule.manual_shortcut_allowed,
                rule.word_count_trigger_allowed,
                rule.character_trigger_allowed,
                rule.local_engine_allowed,
                rule.api_engine_allowed
            ],
        )?;
        Ok(())
    }

    pub(crate) fn list(&self) -> Result<Vec<AppRule>> {
        let mut statement = self.connection.prepare(
            "
            select process_name, window_title_pattern, list_behavior, manual_shortcut_allowed,
                   word_count_trigger_allowed, character_trigger_allowed, local_engine_allowed,
                   api_engine_allowed
            from app_rules
            order by process_name, window_title_pattern
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AppRule {
                process_name: row.get(0)?,
                window_title_pattern: stored_window_title_pattern(row.get(1)?),
                list_behavior: row.get(2)?,
                manual_shortcut_allowed: row.get(3)?,
                word_count_trigger_allowed: row.get(4)?,
                character_trigger_allowed: row.get(5)?,
                local_engine_allowed: row.get(6)?,
                api_engine_allowed: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub(crate) fn delete(
        &self,
        process_name: &str,
        window_title_pattern: Option<&str>,
    ) -> Result<bool> {
        let affected = self.connection.execute(
            "
            delete from app_rules
            where lower(process_name) = lower(?1)
              and (
                  window_title_pattern is ?2
                  or window_title_pattern = ?2
              )
            ",
            params![process_name, window_title_pattern.unwrap_or("")],
        )?;
        Ok(affected > 0)
    }

    pub(crate) fn reset_to_defaults(&self) -> Result<()> {
        self.connection.execute("delete from app_rules", [])?;
        migrations::seed_default_app_rules(self.connection)
    }
}

fn stored_window_title_pattern(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(crate) struct CustomDictionaryRepository<'a> {
    connection: &'a Connection,
}

impl<'a> CustomDictionaryRepository<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn add_entry(&self, entry: &CustomDictionaryEntry) -> Result<()> {
        self.connection.execute(
            "
            insert or ignore into custom_dictionary_entries (language_code, app_process_name, entry)
            values (?1, ?2, ?3)
            ",
            params![entry.language_code, entry.app_process_name, entry.entry],
        )?;
        Ok(())
    }

    pub(crate) fn contains(
        &self,
        language_code: &str,
        app_process_name: Option<&str>,
        entry: &str,
    ) -> Result<bool> {
        let found: i64 = self.connection.query_row(
            "
            select exists(
                select 1 from custom_dictionary_entries
                where language_code = ?1
                  and entry = ?3
                  and (app_process_name is null or app_process_name = ?2)
            )
            ",
            params![language_code, app_process_name, entry],
            |row| row.get(0),
        )?;
        Ok(found == 1)
    }
}

pub(crate) struct LearnedRuleRepository<'a> {
    connection: &'a Connection,
}

impl<'a> LearnedRuleRepository<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn add_rule(&self, rule: &LearnedCorrectionRule) -> Result<()> {
        self.connection.execute(
            "
            insert into learned_correction_rules (
                learning_enabled, original_text, rejected_correction, rule_type,
                language_code, app_process_name
            ) values (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                rule.learning_enabled,
                rule.original_text,
                rule.rejected_correction,
                rule.rule_type,
                rule.language_code,
                rule.app_process_name
            ],
        )?;
        Ok(())
    }
}

pub(crate) struct LanguageOverrideRepository<'a> {
    connection: &'a Connection,
}

impl<'a> LanguageOverrideRepository<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn upsert(&self, override_rule: &LanguageOverride) -> Result<()> {
        self.connection.execute(
            "
            insert into language_overrides (app_process_name, language_code)
            values (?1, ?2)
            on conflict(app_process_name) do update set
                language_code = excluded.language_code,
                updated_at = current_timestamp
            ",
            params![override_rule.app_process_name, override_rule.language_code],
        )?;
        Ok(())
    }

    pub(crate) fn find(&self, app_process_name: &str) -> Result<Option<String>> {
        let mut statement = self
            .connection
            .prepare("select language_code from language_overrides where app_process_name = ?1")?;
        let mut rows = statement.query([app_process_name])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }
}
