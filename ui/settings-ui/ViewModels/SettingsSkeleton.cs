using System.Collections.ObjectModel;
using System.Globalization;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public static class SettingsSkeleton
{
    public static ObservableCollection<OptionItem> RunModes() =>
    [
        new("Blocklist", "blocklist"),
        new("Allowlist", "allowlist"),
    ];

    public static ObservableCollection<OptionItem> Modes() =>
    [
        new("Typos only", "typos_only"),
        new("Typos + grammar", "typos_plus_grammar"),
    ];

    public static ObservableCollection<OptionItem> Engines() =>
    [
        new("Local", "local"),
        new("API", "api"),
    ];

    public static ObservableCollection<OptionItem> ConfidenceBehaviors() =>
    [
        new("Do nothing", "do_nothing"),
        new("Suggest", "suggestion"),
        new("Apply silently", "silent"),
    ];

    public static ObservableCollection<SettingsSectionViewModel> CreateSections() =>
        CreateSections(AppConfig.Default());

    public static ObservableCollection<SettingsSectionViewModel> CreateSections(AppConfig config) =>
    [
        Section("General", "Startup and app run scope",
        [
            BackgroundStatus(),
            Toggle("Start with Windows", "Launch background mode after sign-in.", "general.start_with_windows", config.General.StartWithWindows),
            Dropdown("Run mode", "Block listed apps or run only in allowed apps.", "general.run_mode", config.General.RunMode, RunModes()),
        ]),
        Section("Shortcuts", "Hotkeys for correction and undo",
        [
            Hotkey("Correction shortcut", "Manual correction shortcut.", "shortcuts.correct", config.Shortcuts.Correct),
            Hotkey("Undo shortcut", "App-level undo shortcut.", "shortcuts.undo", config.Shortcuts.Undo),
        ]),
        Section("Triggers", "Word-count and character-triggered correction",
        [
            Toggle("Word-count trigger enabled", "Correct after a configured word count.", "triggers.word_count_enabled", config.Triggers.WordCountEnabled),
            Text("Word-count value", "Words before automatic correction.", "triggers.word_count", config.Triggers.WordCount.ToString(CultureInfo.InvariantCulture)),
            Toggle("Character trigger enabled", "Correct after configured characters.", "triggers.character_trigger_enabled", config.Triggers.CharacterTriggerEnabled),
            Text("Trigger characters", "Comma-separated trigger characters.", "triggers.characters", ConfigValue.Join(config.Triggers.Characters)),
        ]),
        Section("Correction", "Mode and confidence behavior",
        [
            Toggle("Correction enabled", "Allow AutoFix to apply corrections.", "correction.enabled", config.Correction.Enabled),
            Dropdown("Correction mode", "Choose typos only or grammar-aware correction.", "correction.mode", config.Correction.Mode, Modes()),
            Dropdown("High confidence behavior", "Behavior when correction confidence is high.", "correction.high_confidence_behavior", config.Correction.HighConfidenceBehavior, ConfidenceBehaviors()),
            Dropdown("Medium confidence behavior", "Behavior when correction confidence is medium.", "correction.medium_confidence_behavior", config.Correction.MediumConfidenceBehavior, ConfidenceBehaviors()),
            Dropdown("Low confidence behavior", "Recommended: do nothing for safety.", "correction.low_confidence_behavior", config.Correction.LowConfidenceBehavior, ConfidenceBehaviors()),
        ]),
        Section("Engines", "Local and API correction providers",
        [
            Dropdown("Engine", "Route correction requests.", "correction.engine", config.Correction.Engine, Engines()),
            Text("API provider preset", "Provider profile name. API keys are not stored here.", "api.provider_preset", config.Api.ProviderPreset),
            Text("API base URL", "Optional OpenAI-compatible endpoint.", "api.base_url", ConfigValue.Text(config.Api.BaseUrl)),
            Text("API model", "Model name used by the API engine.", "api.model", config.Api.Model),
            Text("Manual API timeout (ms)", "Timeout for manual correction requests.", "api.timeout_manual_ms", config.Api.TimeoutManualMs.ToString(CultureInfo.InvariantCulture)),
            Text("Auto API timeout (ms)", "Timeout for automatic correction requests.", "api.timeout_auto_ms", config.Api.TimeoutAutoMs.ToString(CultureInfo.InvariantCulture)),
            Text("API retry count", "Retries before fallback or failure.", "api.retry_count", config.Api.RetryCount.ToString(CultureInfo.InvariantCulture)),
            Toggle("Fallback to local engine", "Use local correction when API is unavailable.", "api.fallback_to_local", config.Api.FallbackToLocal),
            Text("API temperature", "Must be between 0 and 2.", "api.temperature", config.Api.Temperature.ToString("0.###", CultureInfo.InvariantCulture)),
        ]),
        Section("Context", "Editable and informative context limits",
        [
            Text("Initial context words", "Words read before correction.", "context.initial_context_words", config.Context.InitialContextWords.ToString(CultureInfo.InvariantCulture)),
            Text("Initial context boundary chars", "Comma-separated boundary characters.", "context.initial_context_boundary_chars", ConfigValue.Join(config.Context.InitialContextBoundaryChars)),
            Text("Forward movement word limit", "Maximum words after caret movement.", "context.forward_movement_word_limit", config.Context.ForwardMovementWordLimit.ToString(CultureInfo.InvariantCulture)),
            Text("Informative context max chars", "Maximum read-only context characters.", "context.informative_context_max_chars", config.Context.InformativeContextMaxChars.ToString(CultureInfo.InvariantCulture)),
            Text("Informative context min words", "Minimum informative words.", "context.informative_context_min_words", config.Context.InformativeContextMinWords.ToString(CultureInfo.InvariantCulture)),
            Text("Executable context max words", "Maximum editable words in correction scope.", "context.executable_context_max_words", config.Context.ExecutableContextMaxWords.ToString(CultureInfo.InvariantCulture)),
        ]),
        Section("Feedback", "Tray notices and correction feedback",
        [
            Toggle("Tray state enabled", "Show correction state through tray status.", "feedback.tray_state_enabled", config.Feedback.TrayStateEnabled),
            Toggle("Applied notification", "Notify after a correction is applied.", "feedback.show_correction_applied_notification", config.Feedback.ShowCorrectionAppliedNotification),
            Toggle("Show skipped reason", "Explain why a correction did not run.", "feedback.show_skipped_reason", config.Feedback.ShowSkippedReason),
            Toggle("Show medium-confidence suggestions", "Surface suggestions instead of applying automatically.", "feedback.show_medium_confidence_suggestions", config.Feedback.ShowMediumConfidenceSuggestions),
            Toggle("Show blocked-app notice", "Notify when current app is blocked.", "feedback.show_blocked_app_notice", config.Feedback.ShowBlockedAppNotice),
            Toggle("Show timeout notice", "Notify when correction times out.", "feedback.show_timeout_notice", config.Feedback.ShowTimeoutNotice),
        ]),
        Section("Logs / Debug", "Diagnostics and troubleshooting",
        [
            Toggle("Metadata-only logs enabled", "Keep logs free of typed content.", "logging.metadata_only_logs_enabled", config.Logging.MetadataOnlyLogsEnabled),
            Toggle("Debug mode enabled", "Enable diagnostic logging.", "logging.debug_mode_enabled", config.Logging.DebugModeEnabled),
            Toggle("Redacted debug mode enabled", "Allow redacted debug details.", "logging.redacted_debug_mode_enabled", config.Logging.RedactedDebugModeEnabled),
            Toggle("Full-text debug mode enabled", "Developer-only unsafe diagnostic mode.", "logging.full_text_debug_mode_enabled", config.Logging.FullTextDebugModeEnabled),
            Text("Log retention days", "Empty disables retention cleanup.", "logging.log_retention_days", config.Logging.LogRetentionDays?.ToString(CultureInfo.InvariantCulture) ?? ""),
        ]),
        Section("Advanced", "Config import/export",
        [
            ConfigTransfer("Settings import/export", "Import a saved AutoFix config or export the current one."),
        ]),
    ];

    private static SettingsSectionViewModel Section(
        string name,
        string description,
        IEnumerable<SettingCardViewModel> settings)
    {
        var section = new SettingsSectionViewModel { Name = name, Description = description };
        foreach (var setting in settings)
        {
            section.Settings.Add(setting);
        }

        return section;
    }

    private static SettingCardViewModel Toggle(string title, string description, string path, bool value) =>
        new() { Title = title, Description = description, Kind = "Toggle", Path = path, IsEnabled = value };

    private static SettingCardViewModel Dropdown(
        string title,
        string description,
        string path,
        string value,
        ObservableCollection<OptionItem> options) =>
        new() { Title = title, Description = description, Kind = "Dropdown", Path = path, SelectedValue = value, Options = options };

    private static SettingCardViewModel Hotkey(string title, string description, string path, string hotkey) =>
        new() { Title = title, Description = description, Kind = "Hotkey", Path = path, Hotkey = hotkey };

    private static SettingCardViewModel Text(string title, string description, string path, string value) =>
        new() { Title = title, Description = description, Kind = "Text", Path = path, TextValue = value };

    private static SettingCardViewModel ConfigTransfer(string title, string description) =>
        new() { Title = title, Description = description, Kind = "ConfigTransfer" };

    private static SettingCardViewModel BackgroundStatus() =>
        new() { Title = "Background process status", Kind = "BackgroundStatus" };
}
