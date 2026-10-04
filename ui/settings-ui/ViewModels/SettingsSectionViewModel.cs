using System.Collections.ObjectModel;
using System.Collections.Specialized;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

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
    public bool MatchesSearch { get; private set; } = true;
    public int SearchMatchCount => Settings.Count(card => card.IsSearchMatch);
    public bool ShowAppRulesResults => ShowsFeature("app_rules");
    public bool ShowDictionaryResults => ShowsFeature("dictionary");
    public bool ShowApiKeyResults => ShowsFeature("api_key");
    public bool ShowLanguageResults => ShowsFeature("language_detection");
    public bool ShowLogResults => ShowsFeature("metadata_logs");
    public SettingCardViewModel? AppRulesSearchCard => Feature("app_rules");
    public SettingCardViewModel? DictionarySearchCard => Feature("dictionary");
    public SettingCardViewModel? ApiKeySearchCard => Feature("api_key");
    public SettingCardViewModel? LanguageSearchCard => Feature("language_detection");
    public SettingCardViewModel? LogSearchCard => Feature("metadata_logs");

    private SettingCardViewModel? Feature(string target) => Settings.FirstOrDefault(card => card.SearchTarget == target);
    private bool ShowsFeature(string target) => Feature(target)?.IsSearchMatch == true;

    internal void ApplySearch(SettingsSearchQuery query)
    {
        foreach (var card in Settings)
        {
            // Values, dictionary entries, app rules, logs and secrets are deliberately excluded.
            card.IsSearchMatch = query.Matches(Name, card.Title, card.Description, card.Path,
                string.Join(" ", card.Options.Select(option => option.Label)));
        }
        // A section description is a fallback; it must not drown out a precise setting match.
        if (!Settings.Any(card => card.IsSearchMatch) && query.Matches(Name, Description))
            foreach (var card in Settings) card.IsSearchMatch = true;
        MatchesSearch = Settings.Any(card => card.IsSearchMatch);
        foreach (var property in new[] { nameof(MatchesSearch), nameof(SearchMatchCount), nameof(ShowAppRulesResults),
            nameof(ShowDictionaryResults), nameof(ShowApiKeyResults), nameof(ShowLanguageResults), nameof(ShowLogResults) })
            OnPropertyChanged(property);
    }

    private void OnDictionaryChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        OnPropertyChanged(nameof(HasDictionary));
    }
}
