using System.Collections.ObjectModel;
using System.Windows.Input;
using AutoFix.SettingsUi.Commands;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed class OnboardingViewModel : ObservableObject
{
    public const string AddApiKeyNow = "add_api_key_now";
    public const string UseLocalForNow = "use_local_for_now";
    public const string DisableUntilConfigured = "disable_until_configured";

    private readonly Func<AppConfig> buildCurrentConfig;
    private readonly Action<AppConfig> complete;
    private readonly Action addApiKey;
    private readonly bool hasApiKey;
    private string shortcut = "Ctrl+Alt+Space";
    private string mode = "typos_only";
    private string engine = "local";
    private string apiWithoutKeyChoice = UseLocalForNow;
    private bool startWithWindows = true;

    public OnboardingViewModel(
        AppConfig config,
        bool hasApiKey,
        Func<AppConfig> buildCurrentConfig,
        Action<AppConfig> complete,
        Action addApiKey)
    {
        this.hasApiKey = hasApiKey;
        this.buildCurrentConfig = buildCurrentConfig;
        this.complete = complete;
        this.addApiKey = addApiKey;
        shortcut = config.Shortcuts.Correct;
        mode = config.Correction.Mode;
        engine = config.Correction.Engine;
        startWithWindows = true;
        FinishCommand = new RelayCommand(_ => Finish());
        AddApiKeyCommand = new RelayCommand(_ => addApiKey());
    }

    public ObservableCollection<OptionItem> Modes { get; } = SettingsSkeleton.Modes();
    public ObservableCollection<OptionItem> Engines { get; } = SettingsSkeleton.Engines();
    public ObservableCollection<OptionItem> ApiWithoutKeyChoices { get; } =
    [
        new("Use local engine for now", UseLocalForNow),
        new("Add API key now", AddApiKeyNow),
        new("Finish with correction disabled", DisableUntilConfigured),
    ];

    public ICommand FinishCommand { get; }
    public ICommand AddApiKeyCommand { get; }

    public string Shortcut
    {
        get => shortcut;
        set => SetProperty(ref shortcut, value);
    }

    public string Mode
    {
        get => mode;
        set => SetProperty(ref mode, value);
    }

    public string Engine
    {
        get => engine;
        set
        {
            if (SetProperty(ref engine, value))
            {
                OnPropertyChanged(nameof(NeedsApiKeyChoice));
            }
        }
    }

    public string ApiWithoutKeyChoice
    {
        get => apiWithoutKeyChoice;
        set => SetProperty(ref apiWithoutKeyChoice, value);
    }

    public bool StartWithWindows
    {
        get => startWithWindows;
        set => SetProperty(ref startWithWindows, value);
    }

    public bool NeedsApiKeyChoice => Engine == "api" && !hasApiKey;

    private void Finish()
    {
        if (NeedsApiKeyChoice && ApiWithoutKeyChoice == AddApiKeyNow)
        {
            addApiKey();
            return;
        }

        var config = buildCurrentConfig();
        config.Shortcuts.Correct = Shortcut.Trim();
        config.Correction.Mode = Mode;
        config.Correction.Engine = Engine;
        config.Correction.Enabled = true;
        config.General.StartWithWindows = StartWithWindows;
        config.Onboarding.Completed = true;

        if (NeedsApiKeyChoice && ApiWithoutKeyChoice == UseLocalForNow)
        {
            config.Correction.Engine = "local";
        }
        else if (NeedsApiKeyChoice && ApiWithoutKeyChoice == DisableUntilConfigured)
        {
            config.Correction.Enabled = false;
        }

        complete(config);
    }
}
