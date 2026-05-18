use rusqlite::{params, Connection, Result};

use super::types::{CorrectionMetadata, DebugEvent};

pub(crate) struct CorrectionMetadataRepository<'a> {
    connection: &'a Connection,
}

impl<'a> CorrectionMetadataRepository<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn record(&self, metadata: &CorrectionMetadata) -> Result<()> {
        self.connection.execute(
            "
            insert into correction_metadata (
                session_id, trigger_type, confidence_tier, engine_used,
                replacement_method, result_reason, latency_ms
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                metadata.session_id,
                metadata.trigger_type,
                metadata.confidence_tier,
                metadata.engine_used,
                metadata.replacement_method,
                metadata.result_reason,
                metadata.latency_ms
            ],
        )?;
        Ok(())
    }
}

pub(crate) struct DebugEventRepository<'a> {
    connection: &'a Connection,
}

impl<'a> DebugEventRepository<'a> {
    pub(super) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn record(&self, event: &DebugEvent) -> Result<()> {
        let typed_text = event
            .full_debug_enabled
            .then(|| event.typed_text.as_deref())
            .flatten();
        self.connection.execute(
            "
            insert into debug_events (
                session_id, event_type, severity, message, typed_text, full_debug_enabled
            ) values (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                event.session_id,
                event.event_type,
                event.severity,
                event.message,
                typed_text,
                event.full_debug_enabled
            ],
        )?;
        Ok(())
    }
}
