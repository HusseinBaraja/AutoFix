//! Trigger-aware decisions shared by local and API correction engines.

use super::{
    ConfidenceBehavior, ConfidenceBehaviorSettings, ConfidenceTier, CorrectionInput,
    CorrectionOutput, EngineStatus, NoChangeReason, TriggerType,
};

impl ConfidenceBehaviorSettings {
    /// Medium suggestions are manual-only in v1. Low confidence is always
    /// blocked, even for callers that bypass persisted-config validation.
    pub fn behavior_for(
        &self,
        tier: ConfidenceTier,
        trigger: TriggerType,
        suggestion_ui_available: bool,
    ) -> ConfidenceBehavior {
        let configured = match tier {
            ConfidenceTier::High => self.high,
            ConfidenceTier::Medium => self.medium,
            ConfidenceTier::Low => return ConfidenceBehavior::DoNothing,
        };
        if configured == ConfidenceBehavior::Suggestion
            && (!suggestion_ui_available
                || (tier == ConfidenceTier::Medium && trigger != TriggerType::ManualShortcut))
        {
            ConfidenceBehavior::DoNothing
        } else {
            configured
        }
    }
}

pub(super) fn behavior_for(input: &CorrectionInput, tier: ConfidenceTier) -> ConfidenceBehavior {
    input
        .confidence_behavior
        .behavior_for(tier, input.trigger_type, input.suggestion_ui_available)
}

/// Attach an explicit disposition and discard edits that cannot be acted on.
pub(super) fn enforce(input: &CorrectionInput, mut output: CorrectionOutput) -> CorrectionOutput {
    output.behavior = ConfidenceBehavior::DoNothing;
    if !output.changes_needed || output.status != EngineStatus::Completed {
        return output;
    }
    let behavior = behavior_for(input, output.confidence);
    if behavior == ConfidenceBehavior::DoNothing {
        CorrectionOutput::unchanged(
            input.executable_context.clone(),
            output.confidence,
            NoChangeReason::ConfidenceBelowConfiguredBehavior,
            output.engine_latency_ms,
        )
    } else {
        output.behavior = behavior;
        output
    }
}
