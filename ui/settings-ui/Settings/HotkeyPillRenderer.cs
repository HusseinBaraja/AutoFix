using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Effects;

namespace AutoFix.SettingsUi.Settings;

/// <summary>
/// Builds keycap pill UI elements from a hotkey string like "Ctrl+Alt+Space".
/// </summary>
public static class HotkeyPillRenderer
{
    public static void Render(ItemsControl pills)
    {
        pills.Items.Clear();
        var hotkeyValue = pills.Tag as string ?? "";
        if (string.IsNullOrWhiteSpace(hotkeyValue))
        {
            pills.Items.Add(CreatePlaceholder());
            return;
        }

        var parts = hotkeyValue.Split('+', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        for (var i = 0; i < parts.Length; i++)
        {
            if (i > 0)
            {
                pills.Items.Add(CreateSeparator());
            }

            pills.Items.Add(CreateKeycap(parts[i]));
        }
    }

    private static TextBlock CreatePlaceholder() =>
        new()
        {
            Text = "Click to set shortcut",
            Foreground = new SolidColorBrush(Color.FromRgb(0x94, 0xA3, 0xB8)),
            FontSize = 13,
            FontStyle = FontStyles.Italic,
            VerticalAlignment = VerticalAlignment.Center,
        };

    private static Border CreateKeycap(string keyName) =>
        new()
        {
            Background = new SolidColorBrush(Color.FromRgb(0xF3, 0xF4, 0xF8)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(0xC5, 0xCE, 0xDE)),
            BorderThickness = new Thickness(1, 1, 1, 2),
            CornerRadius = new CornerRadius(6),
            Padding = new Thickness(10, 5, 10, 5),
            Margin = new Thickness(0, 0, 2, 0),
            Effect = new DropShadowEffect
            {
                BlurRadius = 2,
                ShadowDepth = 1,
                Opacity = 0.12,
                Direction = 270,
                Color = Color.FromRgb(0x2D, 0x3A, 0x55),
            },
            Child = new TextBlock
            {
                Text = keyName,
                FontFamily = new FontFamily("Consolas, Cascadia Code, Courier New"),
                FontSize = 13,
                FontWeight = FontWeights.SemiBold,
                Foreground = new SolidColorBrush(Color.FromRgb(0x1E, 0x29, 0x3B)),
                VerticalAlignment = VerticalAlignment.Center,
            },
        };

    private static TextBlock CreateSeparator() =>
        new()
        {
            Text = "+",
            Foreground = new SolidColorBrush(Color.FromRgb(0x94, 0xA3, 0xB8)),
            FontSize = 13,
            FontWeight = FontWeights.Medium,
            VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(4, 0, 4, 0),
        };
}
