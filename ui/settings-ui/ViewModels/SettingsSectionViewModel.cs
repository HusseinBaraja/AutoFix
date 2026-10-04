using System.Collections.ObjectModel;
using System.Collections.Specialized;
using AutoFix.SettingsUi.Models;

namespace AutoFix.SettingsUi.ViewModels;

public sealed class SettingsSectionViewModel : ObservableObject
{
    public SettingsSectionViewModel()
    {
        Dictionary.CollectionChanged += OnDictionaryChanged;
    }

    public string Name { get; init; } = "";
    public string Description { get; init; } = "";
    public bool ShowsAppRules { get; init; }
    public bool ShowsDictionary { get; init; }
    public bool ShowsEngines { get; init; }
    public bool ShowsLogs { get; init; }
    public bool ShowsLanguages { get; init; }
    public ObservableCollection<SettingCardViewModel> Settings { get; } = [];
    public ObservableCollection<AppRuleItem> AppRules { get; } = [];
    public ObservableCollection<DictionaryItem> Dictionary { get; } = [];

    public bool HasAppRules => ShowsAppRules;
    public bool HasDictionary => ShowsDictionary || Dictionary.Count > 0;

    private void OnDictionaryChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        OnPropertyChanged(nameof(HasDictionary));
    }
}
