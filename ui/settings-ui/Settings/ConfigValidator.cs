using System.IO;

namespace AutoFix.SettingsUi.Settings;

public static class ConfigValidator
{
    private static readonly HashSet<string> RunModes = ["blocklist", "allowlist"];
    private static readonly HashSet<string> Modes = ["typos_only", "typos_plus_grammar"];
    private static readonly HashSet<string> Engines = ["local", "api"];
    private static readonly HashSet<string> Confidence = ["do_nothing", "suggestion", "silent"];
    private static readonly HashSet<string> UncertainLanguagePolicies = ["high_confidence_typos_only", "do_nothing", "correct_normally"];
    private static readonly HashSet<string> MixedLanguagePolicies = ["disable_correction", "dominant_language_only", "per_token"];

    /// <summary>Validates settings before they are saved or applied.</summary>
    public static void Validate(AppConfig config)
    {
        RequireChoice("general.run_mode", config.General.RunMode, RunModes);
        RequireHotkey("shortcuts.correct", config.Shortcuts.Correct);
        RequireHotkey("shortcuts.undo", config.Shortcuts.Undo);
        if (config.Context.UndoHistorySize is < 1 or > 1000)
        {
            throw Invalid("context.undo_history_size", "must be between 1 and 1000");
        }
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
        if (config.Context.PendingQueueSize is < 1 or > 16)
        {
            throw Invalid("context.pending_queue_size", "must be between 1 and 16");
        }
        RequireChoice("context.pending_queue_full_behavior", config.Context.PendingQueueFullBehavior,
            new HashSet<string> { "skip_new", "cancel_oldest", "merge_newest" });
        ValidateCorrection(config);
        RequireChoice("learning.mode", config.Learning.Mode, new HashSet<string> { "off", "ask", "automatic" });
        RequireChoice("learning.rule", config.Learning.Rule, new HashSet<string> { "dictionary", "pair" });
        ValidateApi(config);
        ValidateLogging(config);
    }

    /// <summary>Checks correction mode, engine, and confidence settings.</summary>
    private static void ValidateCorrection(AppConfig config)
    {
        RequireChoice("correction.mode", config.Correction.Mode, Modes);
        RequireChoice("correction.engine", config.Correction.Engine, Engines);
        RequireChoice("correction.uncertain_language_policy", config.Correction.UncertainLanguagePolicy, UncertainLanguagePolicies);
        RequireChoice("correction.mixed_language_policy", config.Correction.MixedLanguagePolicy, MixedLanguagePolicies);
        if (config.Correction.MixedLanguagePolicy == "per_token" && config.Correction.Engine != "api")
        {
            throw Invalid("correction.mixed_language_policy", "per-token correction requires the API engine");
        }
        if (config.Correction.PreferredLanguage is { } tag && !ValidLanguageTag(tag))
        {
            throw Invalid("correction.preferred_language", "must be a BCP 47 language tag");
        }
        var apps = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var entry in config.Correction.AppLanguageOverrides)
        {
            var parts = entry.Split('=', 2);
            if (parts.Length != 2 || string.IsNullOrWhiteSpace(parts[0])
                || !ValidLanguageTag(parts[1].Trim()) || !apps.Add(parts[0].Trim()))
            {
                throw Invalid("correction.app_language_overrides", "must contain unique process names and valid language tags");
            }
        }
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
        var categories = GrammarCategories.All.Select(category => category.Value).ToHashSet();
        if (config.Correction.EnabledGrammarCategories.Any(category => !categories.Contains(category))
            || config.Correction.EnabledGrammarCategories.Count != config.Correction.EnabledGrammarCategories.Distinct().Count())
        {
            throw Invalid("correction.enabled_grammar_categories", "contains an unknown or duplicate category");
        }
    }

    /// <summary>Checks RFC 5646 structure without requiring IANA-registered subtags.</summary>
    public static bool ValidLanguageTag(string tag)
    {
        string[] grandfathered = [
            "en-GB-oed", "i-ami", "i-bnn", "i-default", "i-enochian", "i-hak", "i-klingon",
            "i-lux", "i-mingo", "i-navajo", "i-pwn", "i-tao", "i-tay", "i-tsu", "sgn-BE-FR",
            "sgn-BE-NL", "sgn-CH-DE", "art-lojban", "cel-gaulish", "no-bok", "no-nyn",
            "zh-guoyu", "zh-hakka", "zh-min", "zh-min-nan", "zh-xiang"
        ];
        if (grandfathered.Contains(tag, StringComparer.OrdinalIgnoreCase))
        {
            return true;
        }
        var parts = tag.Split('-');
        if (parts.Any(part => part.Length is < 1 or > 8 || !part.All(char.IsAsciiLetterOrDigit)))
        {
            return false;
        }
        if (parts[0].Equals("x", StringComparison.OrdinalIgnoreCase))
        {
            return parts.Length > 1;
        }
        if (parts[0].Length is < 2 or > 8 || !parts[0].All(char.IsAsciiLetter))
        {
            return false;
        }
        var index = 1;
        if (parts[0].Length <= 3)
        {
            for (var count = 0; count < 3 && index < parts.Length
                && parts[index].Length == 3 && parts[index].All(char.IsAsciiLetter); count++)
            {
                index++;
            }
        }
        if (index < parts.Length && parts[index].Length == 4 && parts[index].All(char.IsAsciiLetter))
        {
            index++;
        }
        if (index < parts.Length && ((parts[index].Length == 2 && parts[index].All(char.IsAsciiLetter))
            || (parts[index].Length == 3 && parts[index].All(char.IsAsciiDigit))))
        {
            index++;
        }
        var variants = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        while (index < parts.Length && (parts[index].Length >= 5
            || (parts[index].Length == 4 && char.IsAsciiDigit(parts[index][0]))))
        {
            if (!variants.Add(parts[index++]))
            {
                return false;
            }
        }
        var extensions = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        while (index < parts.Length)
        {
            if (parts[index].Equals("x", StringComparison.OrdinalIgnoreCase))
            {
                return index + 1 < parts.Length;
            }
            if (parts[index].Length != 1 || !extensions.Add(parts[index++]))
            {
                return false;
            }
            var payloadStart = index;
            while (index < parts.Length && parts[index].Length >= 2)
            {
                index++;
            }
            if (index == payloadStart)
            {
                return false;
            }
        }
        return true;
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
        if (config.Api.RetryCount is < 0 or > 1)
        {
            throw Invalid("api.retry_count", "must be 0 or 1");
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
