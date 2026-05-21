namespace AutoFix.SettingsUi.ViewModels;

public sealed class SettingCardViewModel : ObservableObject
{
    private bool isEnabled;
    private string selectedValue = "";
    private string hotkey = "";
    private string textValue = "";

    public string Title { get; init; } = "";
    public string Description { get; init; } = "";
    public string Kind { get; init; } = "";

    public bool IsEnabled
    {
        get => isEnabled;
        set => SetProperty(ref isEnabled, value);
    }

    public string SelectedValue
    {
        get => selectedValue;
        set => SetProperty(ref selectedValue, value);
    }

    public string Hotkey
    {
        get => hotkey;
        set => SetProperty(ref hotkey, value);
    }

    public string TextValue
    {
        get => textValue;
        set => SetProperty(ref textValue, value);
    }

    public bool IsToggle => Kind == "Toggle";
    public bool IsDropdown => Kind == "Dropdown";
    public bool IsHotkey => Kind == "Hotkey";
    public bool IsText => Kind == "Text";
}
