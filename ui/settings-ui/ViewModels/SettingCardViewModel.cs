namespace AutoFix.SettingsUi.ViewModels;

using System.Collections.ObjectModel;
using AutoFix.SettingsUi.Models;

public sealed class SettingCardViewModel : ObservableObject
{
    private bool isEnabled;
    private string selectedValue = "";
    private string hotkey = "";
    private string textValue = "";
    private string validationError = "";
    private bool isRecording;
    private string hotkeyConflictMessage = "";

    public string Title { get; init; } = "";
    public string Description { get; init; } = "";
    public string Kind { get; init; } = "";
    public string Path { get; init; } = "";
    public string DefaultHotkey { get; init; } = "";
    public ObservableCollection<OptionItem> Options { get; init; } = [];

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

    public string ValidationError
    {
        get => validationError;
        set
        {
            if (SetProperty(ref validationError, value))
            {
                OnPropertyChanged(nameof(HasValidationError));
            }
        }
    }

    public bool IsRecording
    {
        get => isRecording;
        set => SetProperty(ref isRecording, value);
    }

    public string HotkeyConflictMessage
    {
        get => hotkeyConflictMessage;
        set
        {
            if (SetProperty(ref hotkeyConflictMessage, value))
            {
                OnPropertyChanged(nameof(HasHotkeyConflict));
            }
        }
    }

    public bool HasHotkeyConflict => !string.IsNullOrWhiteSpace(HotkeyConflictMessage);
    public bool HasValidationError => !string.IsNullOrWhiteSpace(ValidationError);
    public bool IsToggle => Kind == "Toggle";
    public bool IsDropdown => Kind == "Dropdown";
    public bool IsHotkey => Kind == "Hotkey";
    public bool IsText => Kind == "Text";
    public bool IsConfigTransfer => Kind == "ConfigTransfer";
    public bool IsBackgroundStatus => Kind == "BackgroundStatus";
    public bool IsRegularSetting => !IsBackgroundStatus;
}
