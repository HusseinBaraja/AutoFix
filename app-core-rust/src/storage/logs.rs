use rusqlite::{params, Connection, Result};

use super::types::{CorrectionMetadata, DebugEvent, DebugLogMode, DebugPayload};

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
                session_id, app_process_name, trigger_type, confidence_tier, engine_used,
                replacement_method, result_reason, latency_ms
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                metadata.session_id,
                metadata.app_process_name,
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
        let debug_mode = match event.mode {
            DebugLogMode::Off => return Ok(()),
            DebugLogMode::Redacted => "redacted",
            DebugLogMode::FullText => "full_text",
        };
        let full_debug_enabled = matches!(event.mode, DebugLogMode::FullText);
        let (redacted_label, typed_text) = match (&event.mode, &event.payload) {
            (DebugLogMode::Redacted, DebugPayload::Redacted { label }) => {
                (Some(label.as_str()), None)
            }
            (DebugLogMode::FullText, DebugPayload::FullText { typed_text }) => {
                (None, Some(typed_text.as_str()))
            }
            _ => (None, None),
        };
        self.connection.execute(
            "
            insert into debug_events (
                session_id, event_type, severity, message, debug_mode, redacted_label, typed_text,
                full_debug_enabled
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                event.session_id,
                event.event_type,
                event.severity,
                event.message,
                debug_mode,
                redacted_label,
                typed_text,
                full_debug_enabled,
            ],
        )?;
        Ok(())
    }
}

pub(crate) struct SafeDebugEvent {
    session_id: Option<String>,
    event_type: String,
    severity: String,
    message: String,
}

impl SafeDebugEvent {
    pub(crate) fn new(
        session_id: Option<String>,
        event_type: impl Into<String>,
        severity: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            event_type: event_type.into(),
            severity: severity.into(),
            message: message.into(),
        }
    }

    pub(crate) fn off(self) -> DebugEvent {
        self.with_payload(DebugLogMode::Off, DebugPayload::None)
    }

    pub(crate) fn redacted(self, label: impl Into<String>) -> DebugEvent {
        self.with_payload(
            DebugLogMode::Redacted,
            DebugPayload::Redacted {
                label: label.into(),
            },
        )
    }

    pub(crate) fn full_text(self, typed_text: impl Into<String>) -> DebugEvent {
        self.with_payload(
            DebugLogMode::FullText,
            DebugPayload::FullText {
                typed_text: typed_text.into(),
            },
        )
    }

    fn with_payload(self, mode: DebugLogMode, payload: DebugPayload) -> DebugEvent {
        DebugEvent {
            session_id: self.session_id,
            event_type: self.event_type,
            severity: self.severity,
            message: self.message,
            mode,
            payload,
        }
    }
}
