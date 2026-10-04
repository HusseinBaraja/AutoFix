using System.Collections.ObjectModel;
using System.Globalization;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public static class SettingsSkeleton
{
    private static readonly ShortcutsConfig DefaultShortcuts = new();
    /// <summary>Offers app blocking or allowlisting as the runtime scope.</summary>
    public static ObservableCollection<OptionItem> RunModes() =>
    [
        new("Blocklist", "blocklist"),
        new("Allowlist", "allowlist"),
    ];

    /// <summary>Offers typo correction with optional grammar edits.</summary>
    public static ObservableCollection<OptionItem> Modes() =>
    [
        new("Typos only", "typos_only"),
        new("Typos + grammar", "typos_plus_grammar"),
    ];

    /// <summary>Offers explicit local or API engine selection.</summary>
    public static ObservableCollection<OptionItem> Engines() =>
    [
        new("Local", "local"),
        new("API", "api"),
    ];

    /// <summary>Lists provider profiles whose keys live in Windows Credential Manager.</summary>
    public static ObservableCollection<OptionItem> ApiProviders() =>
    [
        new("OpenAI compatible", "openai_compatible"),
        new("OpenAI", "openai"),
        new("Groq", "groq"),
        new("DeepSeek", "deepseek"),
        new("Custom endpoint", "custom"),
    ];

    /// <summary>Lists no-action, suggestion, and silent-apply confidence dispositions.</summary>
    public static ObservableCollection<OptionItem> ConfidenceBehaviors() =>
    [
        new("Do nothing", "do_nothing"),
        new("Suggest when available", "suggestion"),
        new("Apply silently", "silent"),
    ];

    /// <summary>Offers conservative typo-only, blocked, or normal handling of unknown text.</summary>
    public static ObservableCollection<OptionItem> UncertainLanguagePolicies() =>
    [
        new("High-confidence typos only", "high_confidence_typos_only"),
        new("Do nothing", "do_nothing"),
        new("Correct normally", "correct_normally"),
    ];

    /// <summary>Offers blocked, dominant-language, or API per-token handling of mixed text.</summary>
    public static ObservableCollection<OptionItem> MixedLanguagePolicies() =>
    [
        new("Disable correction", "disable_correction"),
        new("Correct dominant language only", "dominant_language_only"),
        new("Correct per token (API)", "per_token"),
    ];

    /// <summary>Builds settings sections from the product defaults.</summary>
    public static ObservableCollection<SettingsSectionViewModel> CreateSections() =>
        CreateSections(AppConfig.Default());

    /// <summary>Builds editable cards from saved settings and fixes low confidence to no action.</summary>
    public static ObservableCollection<SettingsSectionViewModel> CreateSections(AppConfig config) =>
    [
        Section("General", "Startup and background status",
        [
            BackgroundStatus(),
            Toggle("Start with Windows", "Launch background mode after sign-in.", "general.start_with_windows", config.General.StartWithWindows),
        ]),
        Section("Shortcuts", "Hotkeys for correction and undo",
        [
            Hotkey("Correction shortcut", "Manual correction shortcut.", "shortcuts.correct", config.Shortcuts.Correct, DefaultShortcuts.Correct),
            Hotkey("Undo shortcut", "App-level undo shortcut.", "shortcuts.undo", config.Shortcuts.Undo, DefaultShortcuts.Undo),
            Text("Undo history entries", "Corrections kept per session in memory (1–1000). Deleted with the session.", "context.undo_history_size", config.Context.UndoHistorySize.ToString(CultureInfo.InvariantCulture)),
        ]),
        Section("Triggers", "Word-count and character-triggered correction",
        [
            Toggle("Word-count trigger enabled", "Correct after a configured word count.", "triggers.word_count_enabled", config.Triggers.WordCountEnabled),
            Text("Word-count value", "Completed-word threshold before automatic correction.", "triggers.word_count", config.Triggers.WordCount.ToString(CultureInfo.InvariantCulture)),
            Toggle("Character trigger enabled", "Correct after configured characters.", "triggers.character_trigger_enabled", config.Triggers.CharacterTriggerEnabled),
            Text("Trigger characters", "Comma-separated trigger characters.", "triggers.characters", ConfigValue.Join(config.Triggers.Characters)),
        ]),
        Section("Correction", "Mode and confidence behavior",
        [
            Toggle("Correction enabled", "Allow AutoFix to apply corrections.", "correction.enabled", config.Correction.Enabled),
            Dropdown("Correction mode", "Choose typos only or grammar-aware correction.", "correction.mode", config.Correction.Mode, Modes()),
            ..GrammarCategorySettings(config),
            Dropdown("High confidence behavior", "Default: apply silently for manual and automatic triggers.", "correction.high_confidence_behavior", config.Correction.HighConfidenceBehavior, ConfidenceBehaviors()),
            Dropdown("Medium confidence behavior", "Default: do nothing. Enable read-only manual suggestion previews under Feedback, or choose Apply silently for manual and automatic corrections. Suggestion acceptance is not available in v1.", "correction.medium_confidence_behavior", config.Correction.MediumConfidenceBehavior, ConfidenceBehaviors()),
            new SettingCardViewModel
            {
                Title = "Low confidence behavior",
                Description = "Always do nothing. Low-confidence corrections are disabled.",
                Kind = "Dropdown",
                Path = "correction.low_confidence_behavior",
                SelectedValue = "do_nothing",
                Options = [new("Do nothing", "do_nothing")],
                IsAvailable = false,
            },
        ]),
        Section("Languages", "Automatic detection, preferred language, and per-app overrides",
        [
            Text("Preferred language", "Optional BCP 47 tag, such as en-US. Empty uses automatic detection. App overrides take priority.", "correction.preferred_language", config.Correction.PreferredLanguage ?? ""),
            Text("App language overrides", "Comma-separated process.exe=language-tag entries, for example notepad.exe=en-US, winword.exe=ar. Apps without an override use the global preference or automatic detection.", "correction.app_language_overrides", ConfigValue.Join(config.Correction.AppLanguageOverrides)),
            Dropdown("Unknown language", "How to handle text whose language is unclear.", "correction.uncertain_language_policy", config.Correction.UncertainLanguagePolicy, UncertainLanguagePolicies()),
            Dropdown("Mixed-language text", "Disable correction, use the dominant language, or correct each token with the API engine.", "correction.mixed_language_policy", config.Correction.MixedLanguagePolicy, MixedLanguagePolicies()),
            Feature("Automatic language detection", "Use automatic detection to clear the global preferred language. Per-app language overrides still take priority.", "language_detection"),
        ]),
        DictionarySection(config),
        Section("Engines", "Local and API correction providers",
        [
            Dropdown("Engine", "Route correction requests.", "correction.engine", config.Correction.Engine, Engines()),
            Dropdown("API provider preset", "Choose a provider. Store its key in Windows Credential Manager.", "api.provider_preset", config.Api.ProviderPreset, ApiProviders()),
            Text("API base URL", "Optional OpenAI-compatible endpoint.", "api.base_url", ConfigValue.Text(config.Api.BaseUrl)),
            Text("API model", "Model name used by the API engine.", "api.model", config.Api.Model),
            Text("Manual API timeout (ms)", "Timeout for manual correction requests.", "api.timeout_manual_ms", config.Api.TimeoutManualMs.ToString(CultureInfo.InvariantCulture)),
            Text("Auto API timeout (ms)", "Timeout for automatic correction requests.", "api.timeout_auto_ms", config.Api.TimeoutAutoMs.ToString(CultureInfo.InvariantCulture)),
            Text("API retry count", "0 or 1 retry within the same timeout budget.", "api.retry_count", config.Api.RetryCount.ToString(CultureInfo.InvariantCulture)),
            Toggle("Fallback to local engine", "Use local correction when API is unavailable.", "api.fallback_to_local", config.Api.FallbackToLocal),
            Text("API temperature", "Must be between 0 and 2.", "api.temperature", config.Api.Temperature.ToString("0.###", CultureInfo.InvariantCulture)),
            Feature("API key management", "Save, replace or remove a provider key through a masked editor backed by Windows Credential Manager secure storage. Keys are never revealed or included in config exports.", "api_key"),
        ]),
        Section("Context", "Editable and informative context limits",
        [
            Text("Initial context words", "Words read before correction.", "context.initial_context_words", config.Context.InitialContextWords.ToString(CultureInfo.InvariantCulture)),
            Text("Initial context boundary chars", "Comma-separated sentence boundaries used for capture and shrinking.", "context.initial_context_boundary_chars", ConfigValue.Join(config.Context.InitialContextBoundaryChars)),
            Text("Forward movement word limit", "Maximum words after caret movement.", "context.forward_movement_word_limit", config.Context.ForwardMovementWordLimit.ToString(CultureInfo.InvariantCulture)),
            Text("Informative context max chars", "Maximum read-only context characters.", "context.informative_context_max_chars", config.Context.InformativeContextMaxChars.ToString(CultureInfo.InvariantCulture)),
            Text("Informative context min words", "Recent words preserved during shrinking when they fit the character budget.", "context.informative_context_min_words", config.Context.InformativeContextMinWords.ToString(CultureInfo.InvariantCulture)),
            Text("Executable context max words", "Maximum editable words in correction scope.", "context.executable_context_max_words", config.Context.ExecutableContextMaxWords.ToString(CultureInfo.InvariantCulture)),
            Text("Pending correction queue size", "Advanced: running and waiting corrections per session, from 1 to 16. Default: 1.", "context.pending_queue_size", config.Context.PendingQueueSize.ToString(CultureInfo.InvariantCulture)),
            Dropdown("When the pending queue is full", "Skip the new trigger; cancel the oldest; or merge the newest pending segment with current typing and wait for the next trigger.", "context.pending_queue_full_behavior", config.Context.PendingQueueFullBehavior,
                [new("Skip new correction", "skip_new"), new("Cancel oldest correction", "cancel_oldest"), new("Merge newest and wait", "merge_newest")]),
        ]),
        Section("Privacy & Security", "Secure fields, correction scope, and clipboard privacy",
        [
            Information("Password and secure fields", "Password fields and secure controls are always blocked. This protection cannot be disabled."),
            Information("Typed text and caret safety", "AutoFix corrects text typed in the current session by default. Informative context is read-only. Text after the caret is never changed. App-level undo is preserved."),
            Toggle("Correct arbitrary selection", "Allow the manual shortcut to correct selected text outside text typed in this session. Secure-field blocking and pre-caret selection validation still apply.", "shortcuts.correct_arbitrary_selection", config.Shortcuts.CorrectArbitrarySelection),
            Toggle("Use clipboard for correction", "Temporarily paste corrections through the clipboard and restore its previous contents. Turn off to use other replacement methods.", "replacement.clipboard_enabled", config.Replacement.ClipboardEnabled),
        ]),
        Section("Feedback", "Tray notices and correction feedback",
        [
            Toggle("Tray state enabled", "Show idle, active, correcting, blocked and error states. The tray icon always remains available.", "feedback.tray_state_enabled", config.Feedback.TrayStateEnabled),
            Toggle("Applied notification", "Notify after a correction is applied.", "feedback.show_correction_applied_notification", config.Feedback.ShowCorrectionAppliedNotification),
            Toggle("Show skipped reason", "Explain why a correction did not run.", "feedback.show_skipped_reason", config.Feedback.ShowSkippedReason),
            Toggle("Show medium-confidence suggestions", "Show a brief read-only preview for manual medium-confidence suggestions. Never applies the suggestion; acceptance is not available in v1.", "feedback.show_medium_confidence_suggestions", config.Feedback.ShowMediumConfidenceSuggestions),
            Toggle("Show blocked-app notice", "Show a small notice when a manual action is blocked by app policy. Automatic triggers stay silent.", "feedback.show_blocked_app_notice", config.Feedback.ShowBlockedAppNotice),
            Toggle("Show timeout notice", "Show a small notice when a manual API correction times out. Automatic timeouts stay silent.", "feedback.show_timeout_notice", config.Feedback.ShowTimeoutNotice),
            Toggle("Show near-caret overlay", "Place enabled notices near the caret when Windows exposes its position; otherwise use the screen corner. Does not enable extra notices.", "feedback.show_near_caret_overlay", config.Feedback.ShowNearCaretOverlay),
        ]),
        AppRulesSection(config),
        Section("Logs / Debug", "Diagnostics and troubleshooting",
        [
            Toggle("Metadata-only logs enabled", "Keep logs free of typed content.", "logging.metadata_only_logs_enabled", config.Logging.MetadataOnlyLogsEnabled),
            Toggle("Debug mode enabled", "Enable diagnostic logging.", "logging.debug_mode_enabled", config.Logging.DebugModeEnabled),
            Toggle("Redacted debug mode enabled", "Allow redacted debug details.", "logging.redacted_debug_mode_enabled", config.Logging.RedactedDebugModeEnabled),
            Toggle("Full-text debug mode enabled", "May store private typed text on this computer. Requires explicit consent. Disable after troubleshooting and clear logs.", "logging.full_text_debug_mode_enabled", config.Logging.FullTextDebugModeEnabled),
            Text("Log retention days", "Empty disables retention cleanup.", "logging.log_retention_days", config.Logging.LogRetentionDays?.ToString(CultureInfo.InvariantCulture) ?? ""),
            Feature("Correction metadata log viewer", "View or refresh the latest metadata events and clear logs. The viewer never reads typed text or debug payloads. Clear logs removes metadata and debug events after confirmation.", "metadata_logs"),
        ]),
        Section("Advanced", "Config import/export",
        [
            ConfigTransfer("Settings import/export", "Preview and import settings, app rules, dictionary and language overrides. Export a ZIP bundle without API keys, logs or session data."),
        ]),
    ];

    private static SettingsSectionViewModel Section(
        string name,
        string description,
        IEnumerable<SettingCardViewModel> settings)
    {
        var section = new SettingsSectionViewModel { Name = name, Description = description,
            ShowsEngines = name == "Engines", ShowsLogs = name == "Logs / Debug", ShowsLanguages = name == "Languages" };
        foreach (var setting in settings)
        {
            section.Settings.Add(setting);
        }

        return section;
    }

    /// <summary>Creates a boolean card bound to its config field.</summary>
    private static SettingCardViewModel Toggle(string title, string description, string path, bool value) =>
        new() { Title = title, Description = description, Kind = "Toggle", Path = path, IsEnabled = value };

    /// <summary>Builds category switches available only when grammar mode is enabled.</summary>
    private static IEnumerable<SettingCardViewModel> GrammarCategorySettings(AppConfig config)
    {
        foreach (var category in GrammarCategories.All)
        {
            yield return new SettingCardViewModel
            {
                Title = category.Label,
                Description = category.Description,
                Kind = "Toggle",
                Path = $"correction.enabled_grammar_categories.{category.Value}",
                IsEnabled = config.Correction.Mode == "typos_only"
                    || config.Correction.EnabledGrammarCategories.Contains(category.Value),
                IsAvailable = config.Correction.Mode == "typos_plus_grammar",
            };
        }
    }

    private static SettingCardViewModel Dropdown(
        string title,
        string description,
        string path,
        string value,
        ObservableCollection<OptionItem> options) =>
        new() { Title = title, Description = description, Kind = "Dropdown", Path = path, SelectedValue = value, Options = options };

    private static SettingCardViewModel Hotkey(string title, string description, string path, string hotkey, string defaultHotkey) =>
        new() { Title = title, Description = description, Kind = "Hotkey", Path = path, Hotkey = hotkey, DefaultHotkey = defaultHotkey };

    private static SettingCardViewModel Text(string title, string description, string path, string value) =>
        new() { Title = title, Description = description, Kind = "Text", Path = path, TextValue = value };

    private static SettingCardViewModel ConfigTransfer(string title, string description) =>
        new() { Title = title, Description = description, Kind = "ConfigTransfer" };

    private static SettingCardViewModel BackgroundStatus() =>
        new() { Title = "Background process status", Kind = "BackgroundStatus" };

    private static SettingCardViewModel Feature(string title, string description, string target) =>
        new() { Title = title, Description = description, Kind = "Feature", SearchTarget = target };

    private static SettingCardViewModel Information(string title, string description) =>
        new() { Title = title, Description = description, Kind = "Information" };

    private static SettingsSectionViewModel AppRulesSection(AppConfig config)
    {
        var section = new SettingsSectionViewModel { Name = "App Rules",
            Description = "Per-app correction scope, triggers, and engine permissions", ShowsAppRules = true };
        section.Settings.Add(Dropdown("Run mode", "Blocklist runs in unlisted apps. Allowlist runs only in apps explicitly allowed below. Secure fields remain blocked in both modes.", "general.run_mode", config.General.RunMode, RunModes()));
        section.Settings.Add(Feature("App rule table and per-trigger overrides", "Add or remove apps by process and window title pattern. Configure blocklist or allowlist, manual shortcut, word-count and character triggers, local/API permissions and editor prose. Defaults disable terminal/editor triggers and block sensitive apps. Manual correction requires selected prose; editors also require Editor prose. Commands and code are skipped. Delete and add a rule to change its scope.", "app_rules"));
        return section;
    }

    private static SettingsSectionViewModel DictionarySection(AppConfig config)
    {
        var section = new SettingsSectionViewModel { Name = "Dictionary", Description = "Words, phrases, and rejected correction pairs", ShowsDictionary = true };
        section.Settings.Add(Dropdown("Learn from undo", "Off by default. App-level undo rejects a correction. Ask requires your consent before saving text.", "learning.mode", config.Learning.Mode,
            [new("Off — undo only", "off"), new("Ask after undo", "ask"), new("Automatically learn", "automatic")]));
        section.Settings.Add(Dropdown("Learned exclusion", "Protect the original word or phrase, or block only the rejected replacement.", "learning.rule", config.Learning.Rule,
            [new("Never change this to that", "pair"), new("Never correct the original", "dictionary")]));
        section.Settings.Add(Toggle("Learn for this app only", "Limit new learned entries to the app where you undid the correction.", "learning.per_app", config.Learning.PerApp));
        section.Settings.Add(Feature("Protected words and phrases", "Add, edit or remove protected dictionary words, phrases and rejected correction pairs. Choose per-language BCP 47 tags (en, en-US); und protects all languages. Optional per-app process scopes limit exclusions to one app; empty scope applies everywhere. Refresh saved exclusions.", "dictionary"));
        return section;
    }
}
