using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Settings;

public static class ConfigFormMapper
{
    public static AppConfig BuildConfig(IEnumerable<SettingsSectionViewModel> sections)
    {
        var values = sections
            .SelectMany(section => section.Settings)
            .Where(setting => !string.IsNullOrWhiteSpace(setting.Path))
            .ToDictionary(setting => setting.Path);

        var config = AppConfig.Default();
        config.General.StartWithWindows = Toggle(values, "general.start_with_windows");
        config.General.RunMode = Dropdown(values, "general.run_mode");
        config.Shortcuts.Correct = Hotkey(values, "shortcuts.correct");
        config.Shortcuts.Undo = Hotkey(values, "shortcuts.undo");
        config.Triggers.WordCountEnabled = Toggle(values, "triggers.word_count_enabled");
        config.Triggers.WordCount = Int(values, "triggers.word_count");
        config.Triggers.CharacterTriggerEnabled = Toggle(values, "triggers.character_trigger_enabled");
        config.Triggers.Characters = List(values, "triggers.characters");
        ApplyContext(config, values);
        ApplyCorrection(config, values);
        ApplyApi(config, values);
        ApplyFeedback(config, values);
        ApplyLogging(config, values);
        ConfigValidator.Validate(config);
        return config;
    }

    public static void ClearValidation(IEnumerable<SettingsSectionViewModel> sections)
    {
        foreach (var setting in sections.SelectMany(section => section.Settings))
        {
            setting.ValidationError = "";
        }
    }

    public static void MarkValidationError(IEnumerable<SettingsSectionViewModel> sections, string message)
    {
        var field = message.Split(':', 2)[0];
        var setting = sections.SelectMany(section => section.Settings).FirstOrDefault(s => s.Path == field);
        if (setting is not null)
        {
            setting.ValidationError = message;
        }
    }

    private static void ApplyContext(AppConfig config, IReadOnlyDictionary<string, SettingCardViewModel> values)
    {
        config.Context.InitialContextWords = Int(values, "context.initial_context_words");
        config.Context.InitialContextBoundaryChars = List(values, "context.initial_context_boundary_chars");
        config.Context.ForwardMovementWordLimit = Int(values, "context.forward_movement_word_limit");
        config.Context.InformativeContextMaxChars = Int(values, "context.informative_context_max_chars");
        config.Context.InformativeContextMinWords = Int(values, "context.informative_context_min_words");
        config.Context.ExecutableContextMaxWords = Int(values, "context.executable_context_max_words");
    }

    private static void ApplyCorrection(AppConfig config, IReadOnlyDictionary<string, SettingCardViewModel> values)
    {
        config.Correction.Mode = Dropdown(values, "correction.mode");
        config.Correction.Engine = Dropdown(values, "correction.engine");
        config.Correction.HighConfidenceBehavior = Dropdown(values, "correction.high_confidence_behavior");
        config.Correction.MediumConfidenceBehavior = Dropdown(values, "correction.medium_confidence_behavior");
        config.Correction.LowConfidenceBehavior = Dropdown(values, "correction.low_confidence_behavior");
    }

    private static void ApplyApi(AppConfig config, IReadOnlyDictionary<string, SettingCardViewModel> values)
    {
        config.Api.ProviderPreset = Text(values, "api.provider_preset");
        config.Api.BaseUrl = NullWhenEmpty(Text(values, "api.base_url"));
        config.Api.Model = Text(values, "api.model");
        config.Api.TimeoutManualMs = Long(values, "api.timeout_manual_ms");
        config.Api.TimeoutAutoMs = Long(values, "api.timeout_auto_ms");
        config.Api.RetryCount = Int(values, "api.retry_count");
        config.Api.FallbackToLocal = Toggle(values, "api.fallback_to_local");
        config.Api.Temperature = ConfigValue.Double(Text(values, "api.temperature"), "api.temperature");
    }

    private static void ApplyFeedback(AppConfig config, IReadOnlyDictionary<string, SettingCardViewModel> values)
    {
        config.Feedback.TrayStateEnabled = Toggle(values, "feedback.tray_state_enabled");
        config.Feedback.ShowCorrectionAppliedNotification = Toggle(values, "feedback.show_correction_applied_notification");
        config.Feedback.ShowSkippedReason = Toggle(values, "feedback.show_skipped_reason");
        config.Feedback.ShowMediumConfidenceSuggestions = Toggle(values, "feedback.show_medium_confidence_suggestions");
        config.Feedback.ShowBlockedAppNotice = Toggle(values, "feedback.show_blocked_app_notice");
        config.Feedback.ShowTimeoutNotice = Toggle(values, "feedback.show_timeout_notice");
    }

    private static void ApplyLogging(AppConfig config, IReadOnlyDictionary<string, SettingCardViewModel> values)
    {
        config.Logging.MetadataOnlyLogsEnabled = Toggle(values, "logging.metadata_only_logs_enabled");
        config.Logging.DebugModeEnabled = Toggle(values, "logging.debug_mode_enabled");
        config.Logging.RedactedDebugModeEnabled = Toggle(values, "logging.redacted_debug_mode_enabled");
        config.Logging.FullTextDebugModeEnabled = Toggle(values, "logging.full_text_debug_mode_enabled");
        config.Logging.LogRetentionDays = ConfigValue.OptionalInt(Text(values, "logging.log_retention_days"), "logging.log_retention_days");
    }

    private static bool Toggle(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => values[path].IsEnabled;
    private static string Dropdown(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => values[path].SelectedValue;
    private static string Hotkey(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => values[path].Hotkey;
    private static string Text(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => values[path].TextValue;
    private static int Int(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => ConfigValue.Int(Text(values, path), path);
    private static long Long(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => ConfigValue.Long(Text(values, path), path);
    private static List<string> List(IReadOnlyDictionary<string, SettingCardViewModel> values, string path) => ConfigValue.Split(Text(values, path));
    private static string? NullWhenEmpty(string value) => string.IsNullOrWhiteSpace(value) ? null : value;
}
