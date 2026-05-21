using System.Collections.ObjectModel;
using AutoFix.SettingsUi.Models;

namespace AutoFix.SettingsUi.ViewModels;

public sealed class SettingsSectionViewModel : ObservableObject
{
    public string Name { get; init; } = "";
    public string Description { get; init; } = "";
    public ObservableCollection<SettingCardViewModel> Settings { get; } = [];
    public ObservableCollection<AppRuleItem> AppRules { get; } = [];
    public ObservableCollection<DictionaryItem> Dictionary { get; } = [];

    public bool HasAppRules => AppRules.Count > 0;
    public bool HasDictionary => Dictionary.Count > 0;
}
