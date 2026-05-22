using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi;

public partial class MainWindow : Window
{
    private readonly MainWindowViewModel viewModel = new();
    private string? hotkeyBeforeRecording;

    public MainWindow()
    {
        InitializeComponent();
        DataContext = viewModel;
    }

    private async void Window_Loaded(object sender, RoutedEventArgs e)
    {
        try
        {
            await viewModel.LoadSettingsAsync();
        }
        catch (Exception exception)
        {
            System.Diagnostics.Debug.WriteLine(exception);
            viewModel.IsBackgroundRunning = false;
            viewModel.StatusTitle = "Status refresh failed.";
            viewModel.StatusDetail = "Unable to load background process status right now. Try again in a moment.";
            MessageBox.Show(
                this,
                "AutoFix could not load the current background process status. You can keep using the window and try refreshing again.",
                "Status refresh failed",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
        }
    }

    private void HotkeyWell_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (sender is not FrameworkElement well)
        {
            return;
        }

        var capture = FindChild<TextBox>(well);
        capture?.Focus();
        Keyboard.Focus(capture);
    }

    private void HotkeyBox_GotKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: SettingCardViewModel vm })
        {
            hotkeyBeforeRecording = vm.Hotkey;
            vm.IsRecording = true;
        }
    }

    private void HotkeyBox_LostKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: SettingCardViewModel vm })
        {
            vm.IsRecording = false;
        }
    }

    private void HotkeyBox_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (sender is not FrameworkElement { DataContext: SettingCardViewModel vm })
        {
            return;
        }

        var key = e.Key == Key.System ? e.SystemKey : e.Key;

        if (key == Key.Escape)
        {
            vm.Hotkey = hotkeyBeforeRecording ?? vm.DefaultHotkey;
            vm.IsRecording = false;
            Keyboard.ClearFocus();
            e.Handled = true;
            return;
        }

        var hotkey = HotkeyFormatter.Format(key, Keyboard.Modifiers);
        if (!string.IsNullOrWhiteSpace(hotkey))
        {
            vm.Hotkey = hotkey;
            vm.IsRecording = false;
            Keyboard.ClearFocus();
            RefreshHotkeyPills(sender as DependencyObject);
        }

        e.Handled = true;
    }

    private void HotkeyClear_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: SettingCardViewModel vm })
        {
            vm.Hotkey = "";
            RefreshHotkeyPills(sender as DependencyObject);
        }
    }

    private void HotkeyResetDefault_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: SettingCardViewModel vm })
        {
            vm.Hotkey = vm.DefaultHotkey;
            RefreshHotkeyPills(sender as DependencyObject);
        }
    }

    private void HotkeyPills_Loaded(object sender, RoutedEventArgs e)
    {
        if (sender is ItemsControl pills)
        {
            HotkeyPillRenderer.Render(pills);
        }
    }

    private void RefreshHotkeyPills(DependencyObject? source)
    {
        if (source is null)
        {
            return;
        }

        var parent = FindAncestor<Border>(source);
        if (parent is null)
        {
            return;
        }

        var pills = FindChild<ItemsControl>(parent);
        if (pills is not null)
        {
            HotkeyPillRenderer.Render(pills);
        }
    }

    private static T? FindChild<T>(DependencyObject parent) where T : DependencyObject
    {
        var count = VisualTreeHelper.GetChildrenCount(parent);
        for (var i = 0; i < count; i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);
            if (child is T found)
            {
                return found;
            }

            var result = FindChild<T>(child);
            if (result is not null)
            {
                return result;
            }
        }

        return null;
    }

    private static T? FindAncestor<T>(DependencyObject child) where T : DependencyObject
    {
        var current = VisualTreeHelper.GetParent(child);
        while (current is not null)
        {
            if (current is T found)
            {
                return found;
            }

            current = VisualTreeHelper.GetParent(current);
        }

        return null;
    }
}
