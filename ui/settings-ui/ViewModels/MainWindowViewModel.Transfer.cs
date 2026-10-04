using System.Windows.Input;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.Settings.Transfer;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel
{
    private readonly ConfigTransferStorage transferStorage;
    private ConfigImportPreview? importPreview;
    private bool includeLearnedRules;

    public ICommand ConfirmImportCommand { get; }
    public ICommand CancelImportCommand { get; }
    public bool IncludeLearnedRules
    {
        get => includeLearnedRules;
        set => SetProperty(ref includeLearnedRules, value);
    }
    public ConfigImportPreview? ImportPreview
    {
        get => importPreview;
        private set
        {
            if (SetProperty(ref importPreview, value))
            {
                OnPropertyChanged(nameof(IsImportPreviewVisible));
                OnPropertyChanged(nameof(CanEditSettings));
            }
        }
    }
    public bool IsImportPreviewVisible => ImportPreview is not null;
    public bool CanEditSettings => !IsImportPreviewVisible;

    private Task ImportConfigAsync()
    {
        var path = fileDialog.PickImportPath();
        if (path is null) return Task.CompletedTask;
        try
        {
            ImportPreview = transferStorage.Preview(path);
            StatusTitle = "Import ready to preview.";
            StatusDetail = "Review the replacements, then apply or cancel.";
        }
        catch (Exception error) when (IsTransferError(error))
        {
            ImportPreview = null;
            StatusTitle = "Import failed.";
            StatusDetail = ConfigTransferStorage.DescribeFailure(error);
        }
        return Task.CompletedTask;
    }

    private async Task ConfirmImportAsync()
    {
        if (ImportPreview is not { } preview) return;
        if (preview.EnablesFullTextDebug && !settingsConsent.ConfirmFullTextDebug())
        {
            CancelImport();
            StatusDetail = "Full-text debug logging was not authorized. Settings are unchanged.";
            return;
        }
        try
        {
            transferStorage.Apply(preview);
        }
        catch (Exception error) when (IsTransferError(error))
        {
            ImportPreview = null;
            StatusTitle = "Import failed.";
            StatusDetail = ConfigTransferStorage.DescribeFailure(error);
            return;
        }

        ImportPreview = null;
        var detail = "";
        try
        {
            ApplyStartupRegistration(preview.Bundle.Settings);
        }
        catch (Exception error) when (IsTransferError(error))
        {
            detail = $"Windows startup registration failed: {error.Message} | ";
        }
        try
        {
            ApplyConfig(preview.Bundle.Settings, false);
            if (preview.Bundle.Data is null) await LoadAppRulesAsync();
            else ReplaceAppRules(appRuleStorage.List());
        }
        catch (Exception error) when (IsTransferError(error))
        {
            detail += $"Settings display could not refresh: {error.Message} | ";
        }
        var reloadDetail = await NotifyReloadAsync();
        StatusTitle = "Settings imported.";
        StatusDetail = $"{preview.SourceName} | {detail}{reloadDetail}";
    }

    private void CancelImport()
    {
        ImportPreview = null;
        StatusTitle = "Import cancelled.";
        StatusDetail = "Settings and rules are unchanged.";
    }

    private Task ExportConfigAsync()
    {
        var path = fileDialog.PickExportPath();
        if (path is null) return Task.CompletedTask;
        try
        {
            var config = ConfigFormMapper.BuildConfig(Sections);
            config.Onboarding.Completed = onboardingCompleted;
            transferStorage.Export(path, config, IncludeLearnedRules);
            StatusTitle = "Settings exported.";
            StatusDetail = path;
        }
        catch (Exception error) when (IsTransferError(error))
        {
            StatusTitle = "Export failed.";
            StatusDetail = error.Message;
        }
        return Task.CompletedTask;
    }

    private static bool IsTransferError(Exception error) => IsConfigError(error)
        || error is System.Text.Json.JsonException or Microsoft.Data.Sqlite.SqliteException
            or InvalidOperationException or System.Security.SecurityException;
}
