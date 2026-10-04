using System.Windows;
using System.Windows.Controls;

namespace AutoFix.TextTargetFixture;

internal static class Program
{
    [STAThread]
    private static void Main()
    {
        var panel = new StackPanel { Margin = new Thickness(24) };
        panel.Children.Add(new TextBlock
        {
            Text = "Type test text using the keyboard. Use synthetic passwords only.\nClose this window when finished; it never saves or submits text.",
            TextWrapping = TextWrapping.Wrap
        });
        AddField(panel, "Single-line TextBox", new TextBox());
        AddField(panel, "Multiline TextBox", new TextBox
        {
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            Height = 100
        });
        AddField(panel, "PasswordBox — must be blocked", new PasswordBox());
        AddField(panel, "Read-only TextBox", new TextBox
        {
            IsReadOnly = true,
            Text = "Read-only teh text"
        });
        var window = new Window
        {
            Title = "AutoFix WPF text fixture",
            Width = 620,
            SizeToContent = SizeToContent.Height,
            Content = panel
        };
        new Application().Run(window);
    }

    private static void AddField(Panel panel, string label, Control field)
    {
        panel.Children.Add(new TextBlock { Text = label, Margin = new Thickness(0, 18, 0, 5) });
        System.Windows.Automation.AutomationProperties.SetName(field, label);
        panel.Children.Add(field);
    }
}
