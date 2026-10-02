using System.Windows.Input;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel
{
    private readonly DictionaryStorage dictionaryStorage;
    private DictionaryItem? selectedDictionary;
    private string dictionaryWord = "", dictionaryLanguage = "und", dictionaryApp = "", dictionaryReplacement = "", dictionaryKind = "dictionary";
    private string dictionaryMessage = "";

    public ICommand NewDictionaryCommand { get; }
    public ICommand SaveDictionaryCommand { get; }
    public ICommand DeleteDictionaryCommand { get; }
    public ICommand RefreshDictionaryCommand { get; }
    public IReadOnlyList<OptionItem> DictionaryKinds { get; } = [new("Never correct word or phrase", "dictionary"), new("Never change X to Y", "pair")];
    public string DictionaryWord { get => dictionaryWord; set => SetProperty(ref dictionaryWord, value); }
    public string DictionaryLanguage { get => dictionaryLanguage; set => SetProperty(ref dictionaryLanguage, value); }
    public string DictionaryApp { get => dictionaryApp; set => SetProperty(ref dictionaryApp, value); }
    public string DictionaryReplacement { get => dictionaryReplacement; set => SetProperty(ref dictionaryReplacement, value); }
    public string DictionaryKind
    {
        get => dictionaryKind;
        set { if (SetProperty(ref dictionaryKind, value)) OnPropertyChanged(nameof(IsPairRule)); }
    }
    public bool IsPairRule => DictionaryKind == "pair";
    public string DictionaryMessage { get => dictionaryMessage; private set => SetProperty(ref dictionaryMessage, value); }
    public DictionaryItem? SelectedDictionary
    {
        get => selectedDictionary;
        set
        {
            if (!SetProperty(ref selectedDictionary, value) || value is null) return;
            DictionaryWord = value.Word; DictionaryLanguage = value.Language;
            DictionaryApp = value.App; DictionaryReplacement = value.Replacement; DictionaryKind = value.Source;
        }
    }

    /// <summary>Resets the editor to a global, all-language word exclusion without saving.</summary>
    private void NewDictionary()
    {
        DictionaryMessage = "";
        SelectedDictionary = null;
        DictionaryWord = ""; DictionaryLanguage = "und"; DictionaryApp = "";
        DictionaryReplacement = ""; DictionaryKind = "dictionary";
    }

    /// <summary>Refreshes the dictionary section and reports storage errors in the editor.</summary>
    private void LoadDictionary()
    {
        try
        {
            var section = Sections.FirstOrDefault(s => s.ShowsDictionary);
            if (section is null) return;
            var entries = dictionaryStorage.List();
            section.Dictionary.Clear();
            foreach (var entry in entries) section.Dictionary.Add(entry);
            NewDictionary();
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            DictionaryMessage = error.Message;
            StatusTitle = "Dictionary load failed."; StatusDetail = error.Message;
        }
    }

    /// <summary>Validates and atomically saves an exclusion, preserving invalid edits for correction.</summary>
    private void SaveDictionary()
    {
        try
        {
            dictionaryStorage.Save(new DictionaryItem { Word = DictionaryWord, Language = DictionaryLanguage,
                App = DictionaryApp, Replacement = DictionaryReplacement, Source = DictionaryKind }, SelectedDictionary);
            LoadDictionary();
            StatusTitle = "Exclusion saved."; StatusDetail = "Applies to future corrections immediately.";
            DictionaryMessage = "Saved. Applies to future corrections immediately.";
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            StatusTitle = "Exclusion not saved."; StatusDetail = error.Message;
            DictionaryMessage = error.Message;
        }
    }

    /// <summary>Deletes the selected persisted exclusion and refreshes the editor on success.</summary>
    private void DeleteDictionary()
    {
        if (SelectedDictionary is null) return;
        try
        {
            dictionaryStorage.Delete(SelectedDictionary);
            LoadDictionary();
            StatusTitle = "Exclusion deleted."; StatusDetail = "The word or phrase may be corrected again.";
            DictionaryMessage = "Deleted. The word or phrase may be corrected again.";
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            StatusTitle = "Exclusion delete failed."; StatusDetail = error.Message;
            DictionaryMessage = error.Message;
        }
    }
}
