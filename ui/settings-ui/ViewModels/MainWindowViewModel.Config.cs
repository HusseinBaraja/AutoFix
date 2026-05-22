using System.ComponentModel;
using System.IO;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.ViewModels;

public sealed partial class MainWindowViewModel
{
    public async Task LoadSettingsAsync()
    {
        try
        {
            ApplyConfig(configStorage.LoadOrCreate(), false);
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
            ApplyConfig(configStorage.LoadOrCreate(), false);
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
            configStorage.Save(config);
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
        UnsubscribeFromSettings();
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

    private void UnsubscribeFromSettings()
    {
        foreach (var setting in Sections.SelectMany(section => section.Settings))
        {
            setting.PropertyChanged -= SettingChanged;
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
            IsDirty = true;
            _ = SaveSettingsAsync();
        }
    }

    private static bool IsConfigError(Exception error) =>
        error is IOException or UnauthorizedAccessException or InvalidDataException or ArgumentException or FormatException;
}
