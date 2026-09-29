using System.IO;

namespace AutoFix.SettingsUi.Settings;

public static class ConfigValidator
{
    private static readonly HashSet<string> RunModes = ["blocklist", "allowlist"];
    private static readonly HashSet<string> Modes = ["typos_only", "typos_plus_grammar"];
    private static readonly HashSet<string> Engines = ["local", "api"];
    private static readonly HashSet<string> Confidence = ["do_nothing", "suggestion", "silent"];

    /// <summary>Validates settings before they are saved or applied.</summary>
    public static void Validate(AppConfig config)
    {
        RequireChoice("general.run_mode", config.General.RunMode, RunModes);
        RequireHotkey("shortcuts.correct", config.Shortcuts.Correct);
        RequireHotkey("shortcuts.undo", config.Shortcuts.Undo);
        if (HotkeyFormatter.Conflicts(config.Shortcuts.Correct, config.Shortcuts.Undo))
        {
            throw Invalid("shortcuts.undo", "must not match correction shortcut");
        }
        RequirePositive("triggers.word_count", config.Triggers.WordCount);
        RequireList("triggers.characters", config.Triggers.Characters);
        RequirePositive("context.initial_context_words", config.Context.InitialContextWords);
        RequireList("context.initial_context_boundary_chars", config.Context.InitialContextBoundaryChars);
        RequirePositive("context.forward_movement_word_limit", config.Context.ForwardMovementWordLimit);
        RequirePositive("context.informative_context_max_chars", config.Context.InformativeContextMaxChars);
        RequirePositive("context.informative_context_min_words", config.Context.InformativeContextMinWords);
        RequirePositive("context.executable_context_max_words", config.Context.ExecutableContextMaxWords);
        ValidateCorrection(config);
        ValidateApi(config);
        ValidateLogging(config);
    }

    /// <summary>Checks correction mode, engine, and confidence settings.</summary>
    private static void ValidateCorrection(AppConfig config)
    {
        RequireChoice("correction.mode", config.Correction.Mode, Modes);
        RequireChoice("correction.engine", config.Correction.Engine, Engines);
        RequireChoice(
            "correction.high_confidence_behavior",
            config.Correction.HighConfidenceBehavior,
            Confidence);
        RequireChoice(
            "correction.medium_confidence_behavior",
            config.Correction.MediumConfidenceBehavior,
            Confidence);
        RequireChoice("correction.low_confidence_behavior", config.Correction.LowConfidenceBehavior, Confidence);
        if (config.Correction.LowConfidenceBehavior != "do_nothing")
        {
            throw Invalid("correction.low_confidence_behavior", "must be do_nothing");
        }
        if (config.Correction.Mode == "typos_only" && config.Correction.EnabledGrammarCategories.Count > 0)
        {
            throw Invalid("correction.enabled_grammar_categories", "must be empty unless grammar mode is enabled");
        }
    }

    /// <summary>Restricts API providers, endpoints, and request settings.</summary>
    private static void ValidateApi(AppConfig config)
    {
        if (config.Api.ProviderPreset is not ("openai_compatible" or "openai" or "groq" or "deepseek" or "custom"))
        {
            throw Invalid("api.provider_preset", "must be a supported provider preset");
        }
        if (config.Api.ProviderPreset == "custom" && string.IsNullOrWhiteSpace(config.Api.BaseUrl))
        {
            throw Invalid("api.base_url", "required for custom provider");
        }
        if (!string.IsNullOrWhiteSpace(config.Api.BaseUrl))
        {
            if (!Uri.TryCreate(config.Api.BaseUrl, UriKind.Absolute, out var url)
                || (url.Scheme != Uri.UriSchemeHttps && !(url.Scheme == Uri.UriSchemeHttp && url.IsLoopback))
                || !string.IsNullOrEmpty(url.UserInfo)
                || !string.IsNullOrEmpty(url.Query)
                || !string.IsNullOrEmpty(url.Fragment))
            {
                throw Invalid("api.base_url", "must be HTTPS or loopback HTTP without credentials");
            }
        }
        RequireText("api.model", config.Api.Model);
        RequirePositive("api.timeout_manual_ms", config.Api.TimeoutManualMs);
        RequirePositive("api.timeout_auto_ms", config.Api.TimeoutAutoMs);
        if (config.Api.RetryCount is < 0 or > 255)
        {
            throw Invalid("api.retry_count", "must be between 0 and 255");
        }
        if (config.Api.Temperature is < 0 or > 2)
        {
            throw Invalid("api.temperature", "must be between 0 and 2");
        }
        if (config.Api.Streaming)
        {
            throw Invalid("api.streaming", "must remain disabled for correction");
        }
    }

    /// <summary>Checks dependencies between diagnostic logging options.</summary>
    private static void ValidateLogging(AppConfig config)
    {
        if (config.Logging.RedactedDebugModeEnabled && !config.Logging.DebugModeEnabled)
        {
            throw Invalid("logging.redacted_debug_mode_enabled", "requires debug_mode_enabled");
        }
        if (config.Logging.FullTextDebugModeEnabled && !config.Logging.DebugModeEnabled)
        {
            throw Invalid("logging.full_text_debug_mode_enabled", "requires debug_mode_enabled");
        }
        if (config.Logging.LogRetentionDays == 0)
        {
            throw Invalid("logging.log_retention_days", "must be empty or greater than zero");
        }
    }

    /// <summary>Requires a value from the field's supported choices.</summary>
    private static void RequireChoice(string field, string value, HashSet<string> allowed)
    {
        if (!allowed.Contains(value))
        {
            throw Invalid(field, $"must be one of: {string.Join(", ", allowed)}");
        }
    }

    /// <summary>Requires a non-empty text value.</summary>
    private static void RequireText(string field, string value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw Invalid(field, "must not be empty");
        }
    }

    /// <summary>Requires a valid shortcut with a modifier.</summary>
    private static void RequireHotkey(string field, string value)
    {
        if (!HotkeyFormatter.IsValid(value))
        {
            throw Invalid(field, "must include a modifier and supported key");
        }
    }

    /// <summary>Requires a non-empty list of non-empty strings.</summary>
    private static void RequireList(string field, IReadOnlyCollection<string> values)
    {
        if (values.Count == 0 || values.Any(string.IsNullOrWhiteSpace))
        {
            throw Invalid(field, "must contain non-empty strings");
        }
    }

    /// <summary>Requires a positive numeric setting.</summary>
    private static void RequirePositive(string field, long value)
    {
        if (value <= 0)
        {
            throw Invalid(field, "must be greater than zero");
        }
    }

    /// <summary>Names the invalid field in a storage validation error.</summary>
    private static InvalidDataException Invalid(string field, string message) =>
        new($"{field}: {message}");
}
