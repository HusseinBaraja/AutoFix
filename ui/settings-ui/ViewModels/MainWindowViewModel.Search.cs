using System.Windows.Input;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel
{
    private string? sectionBeforeSearch;
    private bool wasSearching;

    public ICommand ClearSearchCommand { get; }
    public bool HasSearchQuery => !string.IsNullOrWhiteSpace(SearchText);
    public bool HasNoSearchResults => HasSearchQuery && !Sections.Any(section => section.MatchesSearch);
    public string SearchSummary => !HasSearchQuery ? "Search names, descriptions, thresholds and behaviors (Ctrl+F)."
        : HasNoSearchResults ? "No matching settings. Try fewer words or clear the search."
        : ResultSummary();

    private string ResultSummary()
    {
        var matches = Sections.Sum(section => section.SearchMatchCount);
        var sections = Sections.Count(section => section.MatchesSearch);
        return $"{matches} matching {(matches == 1 ? "setting" : "settings")} in {sections} {(sections == 1 ? "section" : "sections")}";
    }

    private void UpdateSearch()
    {
        if (HasSearchQuery && !wasSearching) sectionBeforeSearch = SelectedSection?.Name;
        var currentSection = SelectedSection;
        var query = new SettingsSearchQuery(SearchText);
        foreach (var section in Sections) section.ApplySearch(query);
        SectionView.Refresh();

        SelectedSection = query.IsEmpty
            ? Sections.FirstOrDefault(section => section == currentSection)
                ?? Sections.FirstOrDefault(section => section.Name == sectionBeforeSearch) ?? Sections.FirstOrDefault()
            : Sections.Where(section => section.MatchesSearch)
                .OrderByDescending(section => section.Settings.Where(card => card.IsSearchMatch)
                    .Select(card => query.Score(card.Title, card.Description, card.Path, section.Name)).DefaultIfEmpty().Max())
                .FirstOrDefault();
        wasSearching = HasSearchQuery;
        OnPropertyChanged(nameof(HasSearchQuery));
        OnPropertyChanged(nameof(HasNoSearchResults));
        OnPropertyChanged(nameof(SearchSummary));
    }
}
