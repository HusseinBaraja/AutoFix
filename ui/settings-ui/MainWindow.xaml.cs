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
        await viewModel.RefreshStatusAsync();
    }
}
