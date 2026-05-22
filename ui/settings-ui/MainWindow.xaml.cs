using System.Windows;
using System.Windows.Input;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi;

public partial class MainWindow : Window
{
    private readonly MainWindowViewModel viewModel = new();

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

    private void HotkeyBox_GotKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        if (sender is System.Windows.Controls.TextBox textBox)
        {
            textBox.SelectAll();
        }
    }

    private void HotkeyBox_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (sender is not System.Windows.Controls.TextBox textBox)
        {
            return;
        }

        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        var hotkey = HotkeyFormatter.Format(key, Keyboard.Modifiers);
        if (!string.IsNullOrWhiteSpace(hotkey))
        {
            textBox.Text = hotkey;
            textBox.GetBindingExpression(System.Windows.Controls.TextBox.TextProperty)?.UpdateSource();
        }

        e.Handled = true;
    }
}
