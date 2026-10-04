using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Security;
using System.Windows.Input;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel
{
    private readonly LogStorage logStorage;
    private readonly ISettingsConsent settingsConsent;
    private bool updatingRelatedSettings;
    private string apiKeyMessage = "", logsMessage = "", newAppProcess = "", newAppWindowTitle = "";

    public ObservableCollection<MetadataLogItem> MetadataLogs { get; } = [];
    public ICommand RefreshLogsCommand { get; }
    public ICommand ClearLogsCommand { get; }
    public ICommand DeleteApiKeyCommand { get; }
    public ICommand RefreshApiKeyCommand { get; }
    public ICommand AutoDetectLanguageCommand { get; }
    public string ApiKeyMessage { get => apiKeyMessage; private set => SetProperty(ref apiKeyMessage, value); }
    public string LogsMessage { get => logsMessage; private set => SetProperty(ref logsMessage, value); }
    public string NewAppProcess { get => newAppProcess; set => SetProperty(ref newAppProcess, value); }
    public string NewAppWindowTitle { get => newAppWindowTitle; set => SetProperty(ref newAppWindowTitle, value); }
    public string LanguageDetectionSummary => string.IsNullOrWhiteSpace(Setting("correction.preferred_language").TextValue)
        ? "Automatic language detection is active (default)."
        : $"Global preferred language: {Setting("correction.preferred_language").TextValue}.";

    private SettingCardViewModel Setting(string path) => Sections.SelectMany(section => section.Settings).Single(card => card.Path == path);

    /// <summary>Secrets never enter a view-model property, TOML, SQLite, or IPC.</summary>
    public void SaveApiKey(SecureString key)
    {
        try
        {
            if (apiKeyStatus is not IApiKeyStore store) throw new InvalidOperationException("Secure key storage is unavailable.");
            store.Save(Setting("api.provider_preset").SelectedValue, key);
            RefreshApiKeyStatus();
            StatusTitle = "API key saved securely.";
            StatusDetail = "Stored in Windows Credential Manager for the selected provider profile.";
        }
        catch (Exception error) when (error is Win32Exception or ArgumentException or InvalidOperationException)
        { ApiKeyMessage = error.Message; }
    }

    private void DeleteApiKey()
    {
        try
        {
            if (apiKeyStatus is not IApiKeyStore store) throw new InvalidOperationException("Secure key storage is unavailable.");
            store.Delete(Setting("api.provider_preset").SelectedValue);
            RefreshApiKeyStatus();
            StatusTitle = "API key removed.";
            StatusDetail = "The selected provider profile no longer has a saved key.";
        }
        catch (Exception error) when (error is Win32Exception or ArgumentException or InvalidOperationException)
        { ApiKeyMessage = error.Message; }
    }

    private void RefreshApiKeyStatus()
    {
        try
        {
            var config = AppConfig.Default();
            config.Api.ProviderPreset = Setting("api.provider_preset").SelectedValue;
            ApiKeyMessage = apiKeyStatus.HasConfiguredApiKey(config)
                ? $"A key is saved for {config.Api.ProviderPreset}. Enter a new key to replace it."
                : $"No key saved for {config.Api.ProviderPreset}.";
        }
        catch (Win32Exception error) { ApiKeyMessage = error.Message; }
    }

    private void LoadLogs()
    {
        try
        {
            var rows = logStorage.List();
            MetadataLogs.Clear();
            foreach (var row in rows) MetadataLogs.Add(row);
            LogsMessage = rows.Count == 0 ? "No correction metadata yet." : $"Showing the latest {rows.Count} corrections. Timestamps are UTC. Typed text is never displayed.";
        }
        catch (Exception error) when (IsAppRulePersistenceError(error)) { LogsMessage = error.Message; }
    }

    private void ClearLogs()
    {
        if (!settingsConsent.ConfirmClearLogs()) return;
        try
        {
            logStorage.Clear();
            LoadLogs();
            LogsMessage = "Correction metadata and debug events cleared. New events may appear while AutoFix is running.";
        }
        catch (Exception error) when (IsAppRulePersistenceError(error)) { LogsMessage = error.Message; }
    }

    /// <summary>Applies coupled edits as one valid config and requires consent before text diagnostics.</summary>
    private bool UpdateRelatedSettings(SettingCardViewModel card)
    {
        updatingRelatedSettings = true;
        try
        {
            if (card.Path == "logging.full_text_debug_mode_enabled" && card.IsEnabled)
            {
                if (!settingsConsent.ConfirmFullTextDebug())
                {
                    card.IsEnabled = false;
                    StatusTitle = "Full-text debug remains off.";
                    return false;
                }
                Setting("logging.debug_mode_enabled").IsEnabled = true;
                Setting("logging.redacted_debug_mode_enabled").IsEnabled = false;
            }
            if (card.Path == "logging.redacted_debug_mode_enabled" && card.IsEnabled)
            {
                Setting("logging.debug_mode_enabled").IsEnabled = true;
                Setting("logging.full_text_debug_mode_enabled").IsEnabled = false;
            }
            if (card.Path == "logging.debug_mode_enabled" && !card.IsEnabled)
            {
                Setting("logging.redacted_debug_mode_enabled").IsEnabled = false;
                Setting("logging.full_text_debug_mode_enabled").IsEnabled = false;
            }
            if (card.Path == "api.provider_preset")
            {
                // A stale custom URL must not override a newly selected named provider.
                if (card.SelectedValue != "custom") Setting("api.base_url").TextValue = "";
                RefreshApiKeyStatus();
            }
            if (card.Path == "correction.preferred_language") OnPropertyChanged(nameof(LanguageDetectionSummary));
            return true;
        }
        finally { updatingRelatedSettings = false; }
    }
}
