using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using AutoFix.Native;
using Microsoft.Win32;

namespace AutoFix.UI;

/// <summary>Follows the Windows light or dark app theme.</summary>
internal static class Theme
{
    public static bool IsDark { get; private set; }

    public static void Apply(ResourceDictionary resources)
    {
        IsDark = SystemUsesDarkTheme();
        var colors = IsDark ? Dark : Light;
        foreach (var (key, value) in colors)
        {
            var brush = new SolidColorBrush((Color)ColorConverter.ConvertFromString(value));
            brush.Freeze();
            resources[key] = brush;
        }
    }

    /// <summary>Matches the window title bar to the theme.</summary>
    public static void StyleWindow(Window window)
    {
        var handle = new WindowInteropHelper(window).Handle;
        int dark = IsDark ? 1 : 0;
        Win32.DwmSetWindowAttribute(handle, 20, ref dark, sizeof(int)); // DWMWA_USE_IMMERSIVE_DARK_MODE
    }

    private static bool SystemUsesDarkTheme()
    {
        using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        return key?.GetValue("AppsUseLightTheme") is int light && light == 0;
    }

    private static readonly (string, string)[] Light =
    [
        ("WindowBrush", "#F4F5F7"),
        ("CardBrush", "#FFFFFF"),
        ("CardBorderBrush", "#E4E6EB"),
        ("TextBrush", "#14161A"),
        ("SubTextBrush", "#5F6672"),
        ("AccentBrush", "#4F46E5"),
        ("OnAccentBrush", "#FFFFFF"),
        ("AccentSoftBrush", "#EEF0FF"),
        ("InputBrush", "#FFFFFF"),
        ("InputBorderBrush", "#D3D6DC"),
        ("HoverBrush", "#F0F1F4"),
        ("ChipBrush", "#EEF0F3"),
        ("SegmentTrackBrush", "#ECEEF1"),
        ("SwitchOffBrush", "#7A808A"),
        ("ScrollThumbBrush", "#C5C9D0"),
        ("SuccessBrush", "#047857"),
        ("DangerBrush", "#C42B1C"),
    ];

    private static readonly (string, string)[] Dark =
    [
        ("WindowBrush", "#16181C"),
        ("CardBrush", "#1F2228"),
        ("CardBorderBrush", "#2C3038"),
        ("TextBrush", "#F2F3F5"),
        ("SubTextBrush", "#A3A9B4"),
        ("AccentBrush", "#8B8CF8"),
        ("OnAccentBrush", "#0E0F14"),
        ("AccentSoftBrush", "#2A2C48"),
        ("InputBrush", "#16181C"),
        ("InputBorderBrush", "#3A3F49"),
        ("HoverBrush", "#2A2E36"),
        ("ChipBrush", "#2A2E36"),
        ("SegmentTrackBrush", "#16181C"),
        ("SwitchOffBrush", "#9AA0AA"),
        ("ScrollThumbBrush", "#454B56"),
        ("SuccessBrush", "#34D399"),
        ("DangerBrush", "#FF8A80"),
    ];
}
