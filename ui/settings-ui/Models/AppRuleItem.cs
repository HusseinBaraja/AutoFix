using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Models;

public sealed class AppRuleItem : ObservableObject
{
    private string processName = "";
    private string windowTitlePattern = "";
    private string listBehavior = "allowlist";
    private bool manualShortcutAllowed = true;
    private bool wordCountTriggerAllowed = true;
    private bool characterTriggerAllowed = true;
    private bool localEngineAllowed = true;
    private bool apiEngineAllowed = true;

    public string ProcessName
    {
        get => processName;
        set => SetProperty(ref processName, value);
    }

    public string WindowTitlePattern
    {
        get => windowTitlePattern;
        set => SetProperty(ref windowTitlePattern, value);
    }

    public string ListBehavior
    {
        get => listBehavior;
        set => SetProperty(ref listBehavior, value);
    }

    public bool ManualShortcutAllowed
    {
        get => manualShortcutAllowed;
        set => SetProperty(ref manualShortcutAllowed, value);
    }

    public bool WordCountTriggerAllowed
    {
        get => wordCountTriggerAllowed;
        set => SetProperty(ref wordCountTriggerAllowed, value);
    }

    public bool CharacterTriggerAllowed
    {
        get => characterTriggerAllowed;
        set => SetProperty(ref characterTriggerAllowed, value);
    }

    public bool LocalEngineAllowed
    {
        get => localEngineAllowed;
        set => SetProperty(ref localEngineAllowed, value);
    }

    public bool ApiEngineAllowed
    {
        get => apiEngineAllowed;
        set => SetProperty(ref apiEngineAllowed, value);
    }

    public AppRuleItem Clone() => new()
    {
        ProcessName = ProcessName,
        WindowTitlePattern = WindowTitlePattern,
        ListBehavior = ListBehavior,
        ManualShortcutAllowed = ManualShortcutAllowed,
        WordCountTriggerAllowed = WordCountTriggerAllowed,
        CharacterTriggerAllowed = CharacterTriggerAllowed,
        LocalEngineAllowed = LocalEngineAllowed,
        ApiEngineAllowed = ApiEngineAllowed,
    };
}
