using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using AutoFix.SettingsUi.Ipc;

namespace AutoFix.SettingsUi;

public partial class MainWindow : Window
{
    private readonly BackgroundIpcClient ipcClient = new();

    public MainWindow()
    {
        InitializeComponent();
    }

    private async void Window_Loaded(object sender, RoutedEventArgs e)
    {
        await RefreshStatusAsync();
    }

    private async void RefreshButton_Click(object sender, RoutedEventArgs e)
    {
        await RefreshStatusAsync();
    }

    private async void ReloadConfigButton_Click(object sender, RoutedEventArgs e)
    {
        await RunStatusCommand(() => ipcClient.ReloadConfigAsync(), "Config reloaded.");
    }

    private async void UpdateModeButton_Click(object sender, RoutedEventArgs e)
    {
        await UpdateSettingAsync("correction.mode", ModeComboBox.SelectedValue?.ToString());
    }

    private async void UpdateEngineButton_Click(object sender, RoutedEventArgs e)
    {
        await UpdateSettingAsync("correction.engine", EngineComboBox.SelectedValue?.ToString());
    }

    private async void ViewLogsButton_Click(object sender, RoutedEventArgs e)
    {
        await RunCommand(async () =>
        {
            var result = await ipcClient.OpenLogsAsync();
            if (result is { Available: true, Error: null, Value: not null })
            {
                OpenFolder(result.Value.LogDirectory);
            }

            return ToMessage(result, "Logs opened.");
        });
    }

    private async void UndoButton_Click(object sender, RoutedEventArgs e)
    {
        await RunCommand(async () => ToMessage(
            await ipcClient.RequestUndoLastCorrectionAsync(),
            "Undo requested."));
    }

    private async void TestEngineButton_Click(object sender, RoutedEventArgs e)
    {
        await RunCommand(async () => ToMessage(
            await ipcClient.TestCorrectionEngineLaterAsync(),
            "Correction engine test queued."));
    }

    private async Task RefreshStatusAsync()
    {
        await RunCommand(async () =>
        {
            var running = await ipcClient.IsBackgroundRunningAsync();
            if (!running.Available || running.Value?.Running != true)
            {
                ApplyUnavailable();
                return "Background process is not running.";
            }

            var status = await ipcClient.GetStatusAsync();
            if (status is { Available: true, Error: null, Value: not null })
            {
                ApplyStatus(status.Value);
                return "Status refreshed.";
            }

            ApplyUnavailable();
            return ToMessage(status, "Status refreshed.");
        });
    }

    private async Task RunStatusCommand(
        Func<Task<IpcResult<AppStatusResponse>>> action,
        string successMessage)
    {
        await RunCommand(async () =>
        {
            var result = await action();
            if (result is { Available: true, Error: null, Value: not null })
            {
                ApplyStatus(result.Value);
            }

            return ToMessage(result, successMessage);
        });
    }

    private async Task UpdateSettingAsync(string path, string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            DetailText.Text = "Select a value first.";
            return;
        }

        await RunCommand(async () =>
        {
            var result = await ipcClient.UpdateSettingAsync(path, value);
            if (result.Error is null)
            {
                await RefreshStatusAsync();
            }

            return ToMessage(result, "Setting updated.");
        });
    }

    private async Task RunCommand(Func<Task<string>> action)
    {
        SetControlsEnabled(false);
        try
        {
            DetailText.Text = await action();
        }
        catch (Exception error) when (error is IOException or TimeoutException or OperationCanceledException)
        {
            ApplyUnavailable();
            DetailText.Text = "Background process is not running.";
        }
        finally
        {
            SetControlsEnabled(true);
        }
    }

    private void ApplyStatus(AppStatusResponse status)
    {
        StatusText.Text = "Background process is running.";
        DetailText.Text = $"Mode: {Label(status.CorrectionMode)} | Engine: {Label(status.Engine)}";
        SelectByTag(ModeComboBox, status.CorrectionMode);
        SelectByTag(EngineComboBox, status.Engine);
    }

    private void ApplyUnavailable()
    {
        StatusText.Text = "Background process unavailable.";
        DetailText.Text = "Start AutoFix background mode, then refresh.";
    }

    private void SetControlsEnabled(bool enabled)
    {
        RefreshButton.IsEnabled = enabled;
        ModeComboBox.IsEnabled = enabled;
        EngineComboBox.IsEnabled = enabled;
    }

    private static string ToMessage<T>(IpcResult<T> result, string successMessage)
    {
        if (!result.Available)
        {
            return "Background process is not running.";
        }

        return result.Error ?? successMessage;
    }

    private static void SelectByTag(ComboBox comboBox, string tag)
    {
        foreach (ComboBoxItem item in comboBox.Items)
        {
            if (item.Tag?.ToString() == tag)
            {
                comboBox.SelectedItem = item;
                return;
            }
        }
    }

    private static string Label(string value) => value switch
    {
        "typos_only" => "Typos only",
        "typos_plus_grammar" => "Typos + grammar",
        "local" => "Local",
        "api" => "API",
        _ => value,
    };

    private static void OpenFolder(string path)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = path,
                UseShellExecute = true,
            });
        }
        catch (Win32Exception)
        {
            Directory.CreateDirectory(path);
            Process.Start(new ProcessStartInfo
            {
                FileName = path,
                UseShellExecute = true,
            });
        }
    }
}
