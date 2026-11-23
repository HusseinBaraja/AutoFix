using System.Text.Json;
using AutoFix.Core.Llm;

namespace AutoFix.Core.Settings;

public enum FixMode
{
    Spelling,
    Grammar,
}

public sealed class ProviderSettings
{
    public string Model { get; set; } = "";
    /// <summary>Only used by the custom provider.</summary>
    public string BaseUrl { get; set; } = "";
}

public sealed class AppSettings
{
    public bool Enabled { get; set; } = true;
    public string Provider { get; set; } = ProviderCatalog.DefaultId;
    public Dictionary<string, ProviderSettings> Providers { get; set; } = new();
    public FixMode Mode { get; set; } = FixMode.Spelling;

    public bool FixOnSentenceEnd { get; set; } = true;
    public bool FixOnPause { get; set; } = true;
    public int PauseMs { get; set; } = 1000;
    public bool FixBeforeEnter { get; set; } = true;
    public string Hotkey { get; set; } = "Ctrl+Alt+Space";

    public bool ShowPopup { get; set; } = true;
    public bool StartWithWindows { get; set; } = true;
    public bool Onboarded { get; set; }

    /// <summary>Process names (for example <c>Code.exe</c>) where AutoFix stays off.</summary>
    public List<string> BlockedApps { get; set; } = [.. DefaultBlockedApps];

    /// <summary>Words AutoFix never changes.</summary>
    public List<string> Dictionary { get; set; } = [];

    /// <summary>Terminals, code editors, password managers and remote desktops, where typed text is not prose.</summary>
    public static readonly IReadOnlyList<string> DefaultBlockedApps =
    [
        "WindowsTerminal.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "conhost.exe", "mintty.exe", "putty.exe",
        "Code.exe", "Cursor.exe", "Windsurf.exe", "devenv.exe", "idea64.exe", "rider64.exe", "pycharm64.exe",
        "webstorm64.exe", "clion64.exe", "goland64.exe", "sublime_text.exe", "zed.exe", "notepad++.exe",
        "1Password.exe", "KeePass.exe", "KeePassXC.exe", "Bitwarden.exe",
        "mstsc.exe", "vmconnect.exe", "vmware.exe", "VirtualBoxVM.exe",
    ];

    public ProviderSettings ProviderSettingsFor(string id)
    {
        if (!Providers.TryGetValue(id, out var settings))
        {
            settings = new ProviderSettings { Model = ProviderCatalog.Get(id).DefaultModel };
            Providers[id] = settings;
        }
        return settings;
    }

    public ProviderConfig ProviderConfig(string? apiKey)
    {
        var preset = ProviderCatalog.Get(Provider);
        var settings = ProviderSettingsFor(preset.Id);
        string baseUrl = preset.IsCustom ? settings.BaseUrl.Trim() : preset.BaseUrl;
        string model = string.IsNullOrWhiteSpace(settings.Model) ? preset.DefaultModel : settings.Model.Trim();
        return new ProviderConfig(preset.Kind, baseUrl, model, apiKey);
    }

    public bool IsBlocked(string? processName) =>
        processName is not null && BlockedApps.Any(app => string.Equals(app, processName, StringComparison.OrdinalIgnoreCase));

    public AppSettings Clone() =>
        JsonSerializer.Deserialize<AppSettings>(JsonSerializer.Serialize(this, SettingsStore.JsonOptions), SettingsStore.JsonOptions)!;
}
