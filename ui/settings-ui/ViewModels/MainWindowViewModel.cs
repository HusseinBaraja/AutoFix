using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows.Data;
using System.Windows.Input;
using AutoFix.SettingsUi.Commands;
using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel : ObservableObject
{
    private readonly IBackgroundIpcClient ipcClient;
    private readonly ConfigStorage configStorage;
    private readonly AppRuleStorage appRuleStorage;
    private readonly IConfigFileDialog fileDialog;
    private readonly IApiKeyStatus apiKeyStatus;
    private readonly IStartupRegistration startupRegistration;
    private SettingsSectionViewModel? selectedSection;
    private AppRuleItem? selectedAppRule;
    private OnboardingViewModel? onboarding;
    private string searchText = "";
    private string statusTitle = "Checking background process...";
    private string statusDetail = "Settings can be edited after AutoFix background mode is running.";
    private bool isBackgroundRunning;
    private bool isDirty;
    private bool isSavingSettings;
    private bool saveSettingsAgain;
    private bool onboardingCompleted;

    public MainWindowViewModel() : this(new BackgroundIpcClient())
    {
    }

    public MainWindowViewModel(BackgroundIpcClient ipcClient)
        : this(ipcClient, new ConfigStorage(), new ConfigFileDialog())
    {
    }

    public MainWindowViewModel(
        IBackgroundIpcClient ipcClient,
        ConfigStorage configStorage,
        IConfigFileDialog fileDialog)
        : this(ipcClient, configStorage, fileDialog, new EnvironmentApiKeyStatus())
    {
    }

    public MainWindowViewModel(
        IBackgroundIpcClient ipcClient,
        ConfigStorage configStorage,
        IConfigFileDialog fileDialog,
        IApiKeyStatus apiKeyStatus)
        : this(ipcClient, configStorage, fileDialog, apiKeyStatus, new WindowsStartupRegistration())
    {
    }

    public MainWindowViewModel(
        IBackgroundIpcClient ipcClient,
        ConfigStorage configStorage,
        IConfigFileDialog fileDialog,
        IApiKeyStatus apiKeyStatus,
        IStartupRegistration startupRegistration)
        : this(ipcClient, configStorage, new AppRuleStorage(), fileDialog, apiKeyStatus, startupRegistration)
    {
    }

    public MainWindowViewModel(
        IBackgroundIpcClient ipcClient,
        ConfigStorage configStorage,
        AppRuleStorage appRuleStorage,
        IConfigFileDialog fileDialog,
        IApiKeyStatus apiKeyStatus,
        IStartupRegistration startupRegistration)
    {
        this.ipcClient = ipcClient;
        this.configStorage = configStorage;
        this.appRuleStorage = appRuleStorage;
        this.fileDialog = fileDialog;
        this.apiKeyStatus = apiKeyStatus;
        this.startupRegistration = startupRegistration;
        Sections = SettingsSkeleton.CreateSections();
        SubscribeToSettings();
        SelectedSection = Sections.FirstOrDefault();
        SectionView = CollectionViewSource.GetDefaultView(Sections);
        SectionView.Filter = FilterSection;

        RefreshStatusCommand = new AsyncRelayCommand(RefreshStatusAsync);
        LaunchBackgroundCommand = new RelayCommand(_ => ShowLaunchPlaceholder());
        ImportConfigCommand = new AsyncRelayCommand(ImportConfigAsync);
        ExportConfigCommand = new AsyncRelayCommand(ExportConfigAsync);
        AddAppRuleCommand = new AsyncRelayCommand(AddAppRuleAsync);
        DeleteAppRuleCommand = new AsyncRelayCommand(DeleteSelectedAppRuleAsync);
        ResetAppRulesCommand = new AsyncRelayCommand(ResetAppRulesAsync);
    }

    public ObservableCollection<SettingsSectionViewModel> Sections { get; }
    public ICollectionView SectionView { get; }
    public ObservableCollection<OptionItem> Modes { get; } = SettingsSkeleton.Modes();
    public ObservableCollection<OptionItem> Engines { get; } = SettingsSkeleton.Engines();
    public ICommand RefreshStatusCommand { get; }
    public ICommand LaunchBackgroundCommand { get; }
    public ICommand ImportConfigCommand { get; }
    public ICommand ExportConfigCommand { get; }
    public ICommand AddAppRuleCommand { get; }
    public ICommand DeleteAppRuleCommand { get; }
    public ICommand ResetAppRulesCommand { get; }

    public OnboardingViewModel? Onboarding
    {
        get => onboarding;
        set
        {
            if (SetProperty(ref onboarding, value))
            {
                OnPropertyChanged(nameof(IsOnboardingVisible));
            }
        }
    }

    public bool IsOnboardingVisible => Onboarding is not null;

    public SettingsSectionViewModel? SelectedSection
    {
        get => selectedSection;
        set => SetProperty(ref selectedSection, value);
    }

    public AppRuleItem? SelectedAppRule
    {
        get => selectedAppRule;
        set => SetProperty(ref selectedAppRule, value);
    }

    public string SearchText
    {
        get => searchText;
        set
        {
            if (SetProperty(ref searchText, value?.Trim() ?? ""))
            {
                SectionView.Refresh();
                SelectBestSearchSection();
            }
        }
    }

    public string StatusTitle
    {
        get => statusTitle;
        set => SetProperty(ref statusTitle, value);
    }

    public string StatusDetail
    {
        get => statusDetail;
        set => SetProperty(ref statusDetail, value);
    }

    public bool IsBackgroundRunning
    {
        get => isBackgroundRunning;
        set => SetProperty(ref isBackgroundRunning, value);
    }

    public bool IsDirty
    {
        get => isDirty;
        set => SetProperty(ref isDirty, value);
    }

    public async Task RefreshStatusAsync()
    {
        try
        {
            var running = await ipcClient.IsBackgroundRunningAsync();
            if (!running.Available || running.Value?.Running != true)
            {
                ApplyUnavailable();
                return;
            }

            var status = await ipcClient.GetStatusAsync();
            if (status is { Available: true, Error: null, Value: not null })
            {
                IsBackgroundRunning = true;
                StatusTitle = "Background process is running.";
                StatusDetail = $"Mode: {Label(status.Value.CorrectionMode)} | Engine: {Label(status.Value.Engine)}";
                return;
            }

            ApplyUnavailable(status.Error);
        }
        catch (Exception error) when (error is IOException or TimeoutException or OperationCanceledException)
        {
            ApplyUnavailable();
        }
    }

    private bool FilterSection(object item)
    {
        if (item is not SettingsSectionViewModel section || string.IsNullOrWhiteSpace(SearchText))
        {
            return true;
        }

        return section.Name.Contains(SearchText, StringComparison.OrdinalIgnoreCase)
            || section.Description.Contains(SearchText, StringComparison.OrdinalIgnoreCase)
            || section.Settings.Any(s => SettingMatches(s, SearchText));
    }

    private void SelectBestSearchSection()
    {
        if (string.IsNullOrWhiteSpace(SearchText))
        {
            return;
        }

        var bestSection = Sections
            .Select(section => new { Section = section, Score = SearchScore(section, SearchText) })
            .Where(match => match.Score > 0)
            .OrderByDescending(match => match.Score)
            .Select(match => match.Section)
            .FirstOrDefault();

        if (bestSection is not null)
        {
            SelectedSection = bestSection;
        }
    }

    private static int SearchScore(SettingsSectionViewModel section, string query)
    {
        var score = MatchScore(section.Name, query) * 100
            + MatchScore(section.Description, query) * 20;

        foreach (var setting in section.Settings)
        {
            score += MatchScore(setting.Title, query) * 50;
            score += MatchScore(setting.Description, query) * 10;
            score += MatchScore(setting.Path, query) * 5;
        }

        return score;
    }

    private static bool SettingMatches(SettingCardViewModel setting, string query) =>
        setting.Title.Contains(query, StringComparison.OrdinalIgnoreCase)
        || setting.Description.Contains(query, StringComparison.OrdinalIgnoreCase)
        || setting.Path.Contains(query, StringComparison.OrdinalIgnoreCase);

    private static int MatchScore(string text, string query)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            return 0;
        }

        if (text.Equals(query, StringComparison.OrdinalIgnoreCase))
        {
            return 4;
        }

        if (text.StartsWith(query, StringComparison.OrdinalIgnoreCase))
        {
            return 3;
        }

        return text.Contains(query, StringComparison.OrdinalIgnoreCase) ? 1 : 0;
    }

    private void ApplyUnavailable(string? detail = null)
    {
        IsBackgroundRunning = false;
        StatusTitle = "Background process unavailable.";
        StatusDetail = detail ?? "Start AutoFix background mode when available, then refresh.";
    }

    private void ShowLaunchPlaceholder()
    {
        ShowPlaceholder("Background launch hook will be wired after process ownership is finalized.");
    }

    private void ShowPlaceholder(string message)
    {
        StatusDetail = message;
        Debug.WriteLine(message);
    }

    private static string Label(string value) => value switch
    {
        "typos_only" => "Typos only",
        "typos_plus_grammar" => "Typos + grammar",
        "local" => "Local",
        "api" => "API",
        _ => value,
    };
}
