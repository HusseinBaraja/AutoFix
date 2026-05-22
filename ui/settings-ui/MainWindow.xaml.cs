using System.Windows;
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
}
