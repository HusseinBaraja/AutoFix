using System.Text.Json.Serialization;

namespace AutoFix.SettingsUi.Settings;

public sealed class AppConfig
{
    [JsonPropertyName("general")]
    public GeneralConfig General { get; set; } = new();

    [JsonPropertyName("shortcuts")]
    public ShortcutsConfig Shortcuts { get; set; } = new();

    [JsonPropertyName("triggers")]
    public TriggersConfig Triggers { get; set; } = new();

    [JsonPropertyName("context")]
    public ContextConfig Context { get; set; } = new();

    [JsonPropertyName("correction")]
    public CorrectionConfig Correction { get; set; } = new();

    [JsonPropertyName("api")]
    public ApiConfig Api { get; set; } = new();

    [JsonPropertyName("feedback")]
    public FeedbackConfig Feedback { get; set; } = new();

    [JsonPropertyName("logging")]
    public LoggingConfig Logging { get; set; } = new();

    public static AppConfig Default() => new();
}

public sealed class GeneralConfig
{
    [JsonPropertyName("start_with_windows")]
    public bool StartWithWindows { get; set; }

    [JsonPropertyName("run_mode")]
    public string RunMode { get; set; } = "blocklist";
}

public sealed class ShortcutsConfig
{
    [JsonPropertyName("correct")]
    public string Correct { get; set; } = "Ctrl+Alt+Space";

    [JsonPropertyName("undo")]
    public string Undo { get; set; } = "Ctrl+Alt+Z";
}

public sealed class TriggersConfig
{
    [JsonPropertyName("word_count_enabled")]
    public bool WordCountEnabled { get; set; } = true;

    [JsonPropertyName("word_count")]
    public int WordCount { get; set; } = 10;

    [JsonPropertyName("character_trigger_enabled")]
    public bool CharacterTriggerEnabled { get; set; } = true;

    [JsonPropertyName("characters")]
    public List<string> Characters { get; set; } = ["."];
}

public sealed class ContextConfig
{
    [JsonPropertyName("initial_context_words")]
    public int InitialContextWords { get; set; } = 25;

    [JsonPropertyName("initial_context_boundary_chars")]
    public List<string> InitialContextBoundaryChars { get; set; } = ["."];

    [JsonPropertyName("forward_movement_word_limit")]
    public int ForwardMovementWordLimit { get; set; } = 5;

    [JsonPropertyName("informative_context_max_chars")]
    public int InformativeContextMaxChars { get; set; } = 2000;

    [JsonPropertyName("informative_context_min_words")]
    public int InformativeContextMinWords { get; set; } = 25;

    [JsonPropertyName("executable_context_max_words")]
    public int ExecutableContextMaxWords { get; set; } = 80;
}

public sealed class CorrectionConfig
{
    [JsonPropertyName("mode")]
    public string Mode { get; set; } = "typos_only";

    [JsonPropertyName("engine")]
    public string Engine { get; set; } = "local";

    [JsonPropertyName("high_confidence_behavior")]
    public string HighConfidenceBehavior { get; set; } = "silent";

    [JsonPropertyName("medium_confidence_behavior")]
    public string MediumConfidenceBehavior { get; set; } = "suggestion";

    [JsonPropertyName("low_confidence_behavior")]
    public string LowConfidenceBehavior { get; set; } = "do_nothing";

    [JsonPropertyName("enabled_grammar_categories")]
    public List<string> EnabledGrammarCategories { get; set; } = [];
}

public sealed class ApiConfig
{
    [JsonPropertyName("provider_preset")]
    public string ProviderPreset { get; set; } = "openai_compatible";

    [JsonPropertyName("base_url")]
    public string? BaseUrl { get; set; }

    [JsonPropertyName("model")]
    public string Model { get; set; } = "gpt-4.1-mini";

    [JsonPropertyName("timeout_manual_ms")]
    public long TimeoutManualMs { get; set; } = 3000;

    [JsonPropertyName("timeout_auto_ms")]
    public long TimeoutAutoMs { get; set; } = 700;

    [JsonPropertyName("retry_count")]
    public int RetryCount { get; set; } = 1;

    [JsonPropertyName("fallback_to_local")]
    public bool FallbackToLocal { get; set; } = true;

    [JsonPropertyName("temperature")]
    public double Temperature { get; set; }

    [JsonPropertyName("streaming")]
    public bool Streaming { get; set; }
}

public sealed class FeedbackConfig
{
    [JsonPropertyName("tray_state_enabled")]
    public bool TrayStateEnabled { get; set; } = true;

    [JsonPropertyName("show_correction_applied_notification")]
    public bool ShowCorrectionAppliedNotification { get; set; }

    [JsonPropertyName("show_skipped_reason")]
    public bool ShowSkippedReason { get; set; } = true;

    [JsonPropertyName("show_medium_confidence_suggestions")]
    public bool ShowMediumConfidenceSuggestions { get; set; } = true;

    [JsonPropertyName("show_blocked_app_notice")]
    public bool ShowBlockedAppNotice { get; set; } = true;

    [JsonPropertyName("show_timeout_notice")]
    public bool ShowTimeoutNotice { get; set; } = true;
}

public sealed class LoggingConfig
{
    [JsonPropertyName("metadata_only_logs_enabled")]
    public bool MetadataOnlyLogsEnabled { get; set; } = true;

    [JsonPropertyName("debug_mode_enabled")]
    public bool DebugModeEnabled { get; set; }

    [JsonPropertyName("redacted_debug_mode_enabled")]
    public bool RedactedDebugModeEnabled { get; set; }

    [JsonPropertyName("full_text_debug_mode_enabled")]
    public bool FullTextDebugModeEnabled { get; set; }

    [JsonPropertyName("log_retention_days")]
    public int? LogRetentionDays { get; set; }
}
