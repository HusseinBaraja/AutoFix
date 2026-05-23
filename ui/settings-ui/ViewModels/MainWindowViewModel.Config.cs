using System.ComponentModel;
using System.IO;
using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel
{
    public async Task LoadSettingsAsync()
    {
        try
        {
            ApplyConfig(configStorage.LoadOrCreate(), false);
            await LoadAppRulesAsync();
            ShowOnboardingIfNeeded();
            StatusTitle = "Settings loaded.";
            StatusDetail = configStorage.ConfigPath;
        }
        catch (Exception error) when (IsConfigError(error))
        {
            StatusTitle = "Settings load failed.";
            StatusDetail = error.Message;
            return;
        }

        await RefreshStatusAsync();
    }

    public async Task SaveSettingsAsync()
    {
        if (isSavingSettings)
        {
            saveSettingsAgain = true;
            return;
        }

        isSavingSettings = true;
        try
        {
            do
            {
                saveSettingsAgain = false;
                await SaveCurrentSettingsAsync();
            }
            while (saveSettingsAgain);
        }
        finally
        {
            isSavingSettings = false;
        }
    }

    private async Task ImportConfigAsync()
    {
        var path = fileDialog.PickImportPath();
        if (path is null)
        {
            return;
        }

        try
        {
            configStorage.Import(path);
            var config = configStorage.LoadOrCreate();
            ApplyStartupRegistration(config);
            ApplyConfig(config, false);
            var reloadDetail = await NotifyReloadAsync();
            StatusTitle = "Settings imported.";
            StatusDetail = $"{path} | {reloadDetail}";
        }
        catch (Exception error) when (IsConfigError(error))
        {
            StatusTitle = "Import failed.";
            StatusDetail = error.Message;
        }
    }

    private Task ExportConfigAsync()
    {
        var path = fileDialog.PickExportPath();
        if (path is null)
        {
            return Task.CompletedTask;
        }

        try
        {
            var config = ConfigFormMapper.BuildConfig(Sections);
            configStorage.Export(path, config);
            StatusTitle = "Settings exported.";
            StatusDetail = path;
        }
        catch (Exception error) when (IsConfigError(error))
        {
            StatusTitle = "Export failed.";
            StatusDetail = error.Message;
        }

        return Task.CompletedTask;
    }

    private async Task<string> NotifyReloadAsync()
    {
        try
        {
            var result = await ipcClient.ReloadConfigAsync();
            if (!result.Available || result.Error is not null)
            {
                return result.Error ?? "Background process unavailable; settings will load on next start.";
            }

            return "Background reload requested.";
        }
        catch (Exception error) when (error is IOException or TimeoutException or OperationCanceledException)
        {
            return "Background process unavailable; settings will load on next start.";
        }
    }

    private async Task SaveCurrentSettingsAsync()
    {
        try
        {
            ConfigFormMapper.ClearValidation(Sections);
            var config = ConfigFormMapper.BuildConfig(Sections);
            config.Onboarding.Completed = onboardingCompleted;
            configStorage.Save(config);
            ApplyStartupRegistration(config);
            IsDirty = false;
            var reloadDetail = await NotifyReloadAsync();
            StatusTitle = "Settings saved automatically.";
            StatusDetail = reloadDetail;
        }
        catch (Exception error) when (IsConfigError(error))
        {
            ConfigFormMapper.MarkValidationError(Sections, error.Message);
            StatusTitle = "Settings not saved.";
            StatusDetail = error.Message;
        }
    }

    private void ApplyConfig(AppConfig config, bool dirty)
    {
        onboardingCompleted = config.Onboarding.Completed;
        UnsubscribeFromSettings();
        UnsubscribeFromAppRules();
        Sections.Clear();
        foreach (var section in SettingsSkeleton.CreateSections(config))
        {
            Sections.Add(section);
        }

        SubscribeToSettings();
        SelectedSection = Sections.FirstOrDefault();
        SectionView.Refresh();
        ConfigFormMapper.ClearValidation(Sections);
        IsDirty = dirty;
    }

    private void ShowOnboardingIfNeeded()
    {
        if (!configStorage.LastLoadCreatedConfig)
        {
            return;
        }

        var config = ConfigFormMapper.BuildConfig(Sections);
        config.Onboarding.Completed = onboardingCompleted;
        if (config.Onboarding.Completed)
        {
            return;
        }

        Onboarding = new OnboardingViewModel(
            config,
            apiKeyStatus.HasConfiguredApiKey(config),
            () => ConfigFormMapper.BuildConfig(Sections),
            CompleteOnboarding,
            ShowApiKeySetup);
    }

    private void CompleteOnboarding(AppConfig config)
    {
        configStorage.Save(config);
        ApplyStartupRegistration(config);
        onboardingCompleted = true;
        ApplyConfig(config, false);
        Onboarding = null;
        StatusTitle = "Setup complete.";
        StatusDetail = "Advanced settings are available for triggers, app rules, languages, privacy, and dictionary.";
        _ = NotifyReloadAsync();
    }

    private void ShowApiKeySetup()
    {
        Onboarding = null;
        SelectedSection = Sections.FirstOrDefault(section => section.Name == "Engines");
        StatusTitle = "Add API key.";
        StatusDetail = "Add an API key through the configured secret store, then choose API again.";
    }

    private void ApplyStartupRegistration(AppConfig config)
    {
        startupRegistration.Apply(config.General.StartWithWindows);
    }

    private void UnsubscribeFromSettings()
    {
        foreach (var setting in Sections.SelectMany(section => section.Settings))
        {
            setting.PropertyChanged -= SettingChanged;
        }
    }

    private async Task LoadAppRulesAsync()
    {
        var rules = await TryListAppRulesFromIpcAsync();
        if (rules is null)
        {
            rules = appRuleStorage.List();
        }

        ReplaceAppRules(rules);
    }

    private async Task<IReadOnlyList<AppRuleItem>?> TryListAppRulesFromIpcAsync()
    {
        try
        {
            var result = await ipcClient.ListAppRulesAsync();
            if (result is { Available: true, Error: null, Value: not null })
            {
                return result.Value.Rules.Select(AppRuleStorage.FromDto).ToList();
            }
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
        }

        return null;
    }

    private void ReplaceAppRules(IEnumerable<AppRuleItem> rules)
    {
        var section = AppRulesSection();
        if (section is null)
        {
            return;
        }

        UnsubscribeFromAppRules();
        section.AppRules.Clear();
        foreach (var rule in rules)
        {
            section.AppRules.Add(rule);
            rule.PropertyChanged += AppRuleChanged;
        }

        SelectedAppRule = section.AppRules.FirstOrDefault();
    }

    private async Task AddAppRuleAsync()
    {
        var section = AppRulesSection();
        if (section is null)
        {
            return;
        }

        var rule = new AppRuleItem { ProcessName = "app.exe" };
        section.AppRules.Add(rule);
        rule.PropertyChanged += AppRuleChanged;
        SelectedAppRule = rule;
        await SaveAppRuleAsync(rule);
    }

    private async Task DeleteSelectedAppRuleAsync()
    {
        var section = AppRulesSection();
        var rule = SelectedAppRule;
        if (section is null || rule is null)
        {
            return;
        }

        var processName = rule.ProcessName;
        var windowTitlePattern = string.IsNullOrWhiteSpace(rule.WindowTitlePattern) ? null : rule.WindowTitlePattern;
        try
        {
            var deleted = await TryDeleteAppRuleFromIpcAsync(processName, windowTitlePattern);
            if (deleted is null)
            {
                deleted = appRuleStorage.Delete(processName, windowTitlePattern);
            }

            rule.PropertyChanged -= AppRuleChanged;
            section.AppRules.Remove(rule);
            SelectedAppRule = section.AppRules.FirstOrDefault();
            StatusTitle = deleted == true ? "App rule deleted." : "App rule removed.";
            StatusDetail = processName;
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            StatusTitle = "App rule delete failed.";
            StatusDetail = error.Message;
        }
    }

    private async Task ResetAppRulesAsync()
    {
        try
        {
            IReadOnlyList<AppRuleItem>? rules = null;
            try
            {
                var result = await ipcClient.ResetAppRulesAsync();
                if (result is { Available: true, Error: null, Value: not null })
                {
                    rules = result.Value.Rules.Select(AppRuleStorage.FromDto).ToList();
                }
            }
            catch (Exception error) when (IsAppRulePersistenceError(error))
            {
            }

            rules ??= appRuleStorage.ResetDefaults();
            ReplaceAppRules(rules);
            StatusTitle = "App rules reset.";
            StatusDetail = "Default safety rules restored.";
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            StatusTitle = "App rules reset failed.";
            StatusDetail = error.Message;
        }
    }

    private async void AppRuleChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (sender is not AppRuleItem rule)
        {
            return;
        }

        await SaveAppRuleAsync(rule);
    }

    private async Task SaveAppRuleAsync(AppRuleItem rule)
    {
        try
        {
            AppRuleStorage.Validate(rule);
            var saved = await TryUpsertAppRuleFromIpcAsync(rule);
            if (!saved)
            {
                appRuleStorage.Upsert(rule);
            }

            StatusTitle = "App rule saved.";
            StatusDetail = rule.ProcessName;
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            StatusTitle = "App rule not saved.";
            StatusDetail = error.Message;
        }
    }

    private async Task<bool> TryUpsertAppRuleFromIpcAsync(AppRuleItem rule)
    {
        try
        {
            var result = await ipcClient.UpsertAppRuleAsync(AppRuleStorage.ToDto(rule));
            return result is { Available: true, Error: null };
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
            return false;
        }
    }

    private async Task<bool?> TryDeleteAppRuleFromIpcAsync(string processName, string? windowTitlePattern)
    {
        try
        {
            var result = await ipcClient.DeleteAppRuleAsync(processName, windowTitlePattern);
            if (result is { Available: true, Error: null, Value: not null })
            {
                return result.Value.Deleted;
            }
        }
        catch (Exception error) when (IsAppRulePersistenceError(error))
        {
        }

        return null;
    }

    private SettingsSectionViewModel? AppRulesSection() =>
        Sections.FirstOrDefault(section => section.ShowsAppRules);

    private void UnsubscribeFromAppRules()
    {
        foreach (var rule in Sections.SelectMany(section => section.AppRules))
        {
            rule.PropertyChanged -= AppRuleChanged;
        }
    }

    private void SubscribeToSettings()
    {
        foreach (var setting in Sections.SelectMany(section => section.Settings))
        {
            setting.PropertyChanged += SettingChanged;
        }
    }

    private void SettingChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName is nameof(SettingCardViewModel.IsEnabled)
            or nameof(SettingCardViewModel.SelectedValue)
            or nameof(SettingCardViewModel.Hotkey)
            or nameof(SettingCardViewModel.TextValue))
        {
            if (args.PropertyName == nameof(SettingCardViewModel.Hotkey))
            {
                ValidateHotkeyConflicts();
            }

            IsDirty = true;
            _ = SaveSettingsAsync();
        }
    }

    internal void ValidateHotkeyConflicts()
    {
        var hotkeys = Sections
            .SelectMany(section => section.Settings)
            .Where(setting => setting.IsHotkey)
            .ToList();

        foreach (var setting in hotkeys)
        {
            setting.HotkeyConflictMessage = "";
        }

        for (var i = 0; i < hotkeys.Count; i++)
        {
            for (var j = i + 1; j < hotkeys.Count; j++)
            {
                if (!HotkeyFormatter.Conflicts(hotkeys[i].Hotkey, hotkeys[j].Hotkey))
                {
                    continue;
                }

                hotkeys[i].HotkeyConflictMessage = $"Conflicts with: {hotkeys[j].Title}";
                hotkeys[j].HotkeyConflictMessage = $"Conflicts with: {hotkeys[i].Title}";
            }
        }
    }

    private static bool IsConfigError(Exception error) =>
        error is IOException or UnauthorizedAccessException or InvalidDataException or ArgumentException or FormatException;

    private static bool IsAppRulePersistenceError(Exception error) =>
        error is IOException or UnauthorizedAccessException or InvalidDataException or ArgumentException or FormatException
            or TimeoutException or OperationCanceledException or Microsoft.Data.Sqlite.SqliteException
            or System.Text.Json.JsonException;
}
