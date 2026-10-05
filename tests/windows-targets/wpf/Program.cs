using System.Windows;
using System.Windows.Controls;
using System.Text.Json;
using System.Windows.Interop;

namespace AutoFix.TextTargetFixture;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        var panel = new StackPanel { Margin = new Thickness(24) };
        panel.Children.Add(new TextBlock
        {
            Text = "Type test text using the keyboard. Use synthetic passwords only.\nClose this window when finished; it never saves or submits text.",
            TextWrapping = TextWrapping.Wrap
        });
        var single = new TextBox();
        AddField(panel, "Single-line TextBox", single);
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
        if (args.Contains("--automation", StringComparer.Ordinal))
        {
            window.Loaded += (_, _) =>
            {
                Console.WriteLine(JsonSerializer.Serialize(new { ready = new WindowInteropHelper(window).Handle.ToInt64() }));
                Console.Out.Flush();
                _ = Task.Run(() => RunCommands(window, single));
            };
        }
        new Application().Run(window);
    }

    private static void AddField(Panel panel, string label, Control field)
    {
        panel.Children.Add(new TextBlock { Text = label, Margin = new Thickness(0, 18, 0, 5) });
        System.Windows.Automation.AutomationProperties.SetName(field, label);
        panel.Children.Add(field);
    }

    // The pipe controls only this disposable fixture, never another application.
    private static void RunCommands(Window window, TextBox field)
    {
        try
        {
            string? line;
            while ((line = Console.ReadLine()) is not null)
            {
                using var request = JsonDocument.Parse(line);
                var command = request.RootElement.GetProperty("command").GetString();
                if (command == "close") { window.Dispatcher.Invoke(window.Close); return; }
                var response = window.Dispatcher.Invoke(() =>
                {
                    if (command == "setup")
                    {
                        field.IsReadOnly = false;
                        field.Text = request.RootElement.GetProperty("text").GetString() ?? "";
                        field.Select(request.RootElement.GetProperty("start").GetInt32(),
                            request.RootElement.GetProperty("length").GetInt32());
                        field.IsReadOnly = request.RootElement.TryGetProperty("readOnly", out var readOnly) && readOnly.GetBoolean();
                        window.Activate();
                        field.Focus();
                    }
                    return JsonSerializer.Serialize(new { text = field.Text, start = field.SelectionStart,
                        length = field.SelectionLength, focused = field.IsKeyboardFocused });
                });
                Console.WriteLine(response);
                Console.Out.Flush();
            }
        }
        finally { if (!window.Dispatcher.HasShutdownStarted) window.Dispatcher.Invoke(window.Close); }
    }
}
