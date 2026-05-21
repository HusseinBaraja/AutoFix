using System.Collections.ObjectModel;
using AutoFix.SettingsUi.Models;

namespace AutoFix.SettingsUi.ViewModels;

public static class SettingsSkeleton
{
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

    public static ObservableCollection<SettingsSectionViewModel> CreateSections() =>
    [
        Section("General", "Startup, tray, theme, and update behavior",
        [
            Toggle("Start AutoFix with Windows", "Launch background mode after sign-in.", true),
            Toggle("Show tray notifications", "Notify when corrections are applied.", true),
            Dropdown("Theme", "Match Windows by default.", "System"),
        ]),
        Section("Shortcuts", "Hotkeys for correction, undo, and quick actions",
        [
            Hotkey("Correct selected text", "Manual correction shortcut.", "Ctrl+Alt+F"),
            Hotkey("Undo last correction", "App-level undo shortcut.", "Ctrl+Alt+Backspace"),
            Toggle("Capture shortcuts globally", "Listen outside the settings window.", true),
        ]),
        Section("Triggers", "Word-count and character-triggered correction",
        [
            Toggle("Enable word-count trigger", "Correct after a configured word count.", true),
            Text("Words before correction", "Placeholder numeric setting.", "8"),
            Text("Character triggers", "Run after punctuation such as period or question mark.", ".?!"),
        ]),
        Section("Correction", "Correction mode, confidence, and caret behavior",
        [
            Dropdown("Correction mode", "Choose typos only or grammar-aware correction.", "typos_only"),
            Text("Minimum confidence", "Placeholder confidence threshold.", "0.72"),
            Toggle("Never change text after caret", "Core safety rule.", true),
        ]),
        Section("Engines", "Local and API correction providers",
        [
            Dropdown("Active engine", "Route correction requests.", "local"),
            Text("API endpoint", "Configured later.", "https://"),
            Toggle("Fallback to local engine", "Use local correction when API is unavailable.", true),
        ]),
        AppRulesSection(),
        Section("Languages", "Detection, mixed-language safeguards, and locale rules",
        [
            Toggle("Detect language automatically", "Detect per correction request.", true),
            Toggle("Block mixed-language rewrites", "Avoid overcorrecting mixed-language input.", true),
            Text("Preferred languages", "Comma-separated placeholder.", "en"),
        ]),
        DictionarySection(),
        Section("Privacy & Security", "Secure fields, clipboard, and telemetry controls",
        [
            Toggle("Block password fields", "Never inspect secure input.", true),
            Toggle("Preserve clipboard", "Restore clipboard after correction.", true),
            Toggle("Send diagnostic telemetry", "Placeholder opt-in setting.", false),
        ]),
        Section("Feedback", "Inline feedback and correction review",
        [
            Toggle("Show correction preview", "Review before applying corrections.", false),
            Toggle("Collect correction feedback", "Store local feedback for tuning.", false),
        ]),
        Section("Logs / Debug", "Diagnostics, logs, and troubleshooting",
        [
            Dropdown("Log level", "Controls background logging detail.", "Info"),
            Toggle("Enable debug overlay", "Developer diagnostic placeholder.", false),
            Text("Log retention days", "Placeholder numeric setting.", "14"),
        ]),
        Section("Advanced", "Config import/export and low-level behavior",
        [
            Toggle("Use allowlist mode", "Only run in approved apps.", false),
            Text("IPC pipe name", "Named pipe used by settings UI.", @"Local\AutoFix.Background.Ipc"),
        ]),
    ];

    private static SettingsSectionViewModel AppRulesSection()
    {
        var section = Section("App Rules", "Per-app allowlists, blocklists, and overrides", []);
        section.AppRules.Add(new AppRuleItem
        {
            App = "Code.exe",
            Scope = "Allow",
            Mode = "Typos only",
            Engine = "Local",
            Notes = "Sample row"
        });
        section.AppRules.Add(new AppRuleItem
        {
            App = "Password fields",
            Scope = "Block",
            Mode = "None",
            Engine = "None",
            Notes = "Secure input"
        });
        return section;
    }

    private static SettingsSectionViewModel DictionarySection()
    {
        var section = Section("Dictionary", "Custom words and correction exceptions", []);
        section.Dictionary.Add(new DictionaryItem { Word = "AutoFix", Language = "en", Source = "Built-in" });
        section.Dictionary.Add(new DictionaryItem { Word = "Zerone", Language = "en", Source = "Custom" });
        return section;
    }

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

    private static SettingCardViewModel Toggle(string title, string description, bool value) =>
        new() { Title = title, Description = description, Kind = "Toggle", IsEnabled = value };

    private static SettingCardViewModel Dropdown(string title, string description, string value) =>
        new()
        {
            Title = title,
            Description = description,
            Kind = "Dropdown",
            SelectedValue = value,
            Options = OptionsFor(title),
        };

    private static SettingCardViewModel Hotkey(string title, string description, string hotkey) =>
        new() { Title = title, Description = description, Kind = "Hotkey", Hotkey = hotkey };

    private static SettingCardViewModel Text(string title, string description, string value) =>
        new() { Title = title, Description = description, Kind = "Text", TextValue = value };

    private static ObservableCollection<OptionItem> OptionsFor(string title) => title switch
    {
        "Theme" =>
        [
            new("System", "System"),
            new("Light", "Light"),
            new("Dark", "Dark"),
        ],
        "Correction mode" => Modes(),
        "Active engine" => Engines(),
        "Log level" =>
        [
            new("Info", "Info"),
            new("Debug", "Debug"),
        ],
        _ => [],
    };
}
