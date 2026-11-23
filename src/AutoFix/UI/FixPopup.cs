using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using AutoFix.Core.Text;
using AutoFix.Native;
using AutoFix.Services;

namespace AutoFix.UI;

/// <summary>
/// A small note near the caret that never takes focus and ignores the mouse,
/// so it cannot interrupt typing.
/// </summary>
internal sealed class FixPopup : INotifier
{
    private readonly Dispatcher _dispatcher;
    private readonly DispatcherTimer _hideTimer;
    private Window? _window;
    private Border _pill = null!;
    private StackPanel _content = null!;
    private bool _working;

    public FixPopup(Dispatcher dispatcher)
    {
        _dispatcher = dispatcher;
        _hideTimer = new DispatcherTimer(DispatcherPriority.Normal, dispatcher);
        _hideTimer.Tick += (_, _) => Hide();
    }

    public void ShowFix(IReadOnlyList<WordChange> changes, string hint, Win32.RECT? anchor) => _dispatcher.BeginInvoke(() =>
    {
        _working = false;
        Fill(content =>
        {
            content.Children.Add(Icon("✓", "#34D399"));
            int shown = 0;
            foreach (var change in changes.Take(3))
            {
                if (shown++ > 0) content.Children.Add(Text("·", "#9CA3AF"));
                if (change.From.Length > 0) content.Children.Add(Text(Shorten(change.From), "#9CA3AF", strike: true));
                content.Children.Add(Text(change.To.Length > 0 ? Shorten(change.To) : "(removed)", "#F9FAFB", bold: true));
            }
            if (changes.Count > 3) content.Children.Add(Text($"+{changes.Count - 3}", "#9CA3AF"));
            if (hint.Length > 0) content.Children.Add(Text(hint, "#9CA3AF", small: true));
        });
        Present(anchor, TimeSpan.FromMilliseconds(hint.StartsWith("Backspace") ? 3500 : 2500));
    });

    public void ShowMessage(string message, Win32.RECT? anchor) => _dispatcher.BeginInvoke(() =>
    {
        _working = false;
        Fill(content =>
        {
            content.Children.Add(Icon("!", "#FBBF24"));
            content.Children.Add(Text(message, "#F9FAFB"));
        });
        Present(anchor, TimeSpan.FromSeconds(4));
    });

    public void ShowWorking(bool working, Win32.RECT? anchor) => _dispatcher.BeginInvoke(() =>
    {
        if (!working)
        {
            if (_working) Hide();
            return;
        }
        _working = true;
        Fill(content =>
        {
            var dot = Icon("●", "#8B8CF8");
            dot.BeginAnimation(UIElement.OpacityProperty,
                new DoubleAnimation(1, 0.25, TimeSpan.FromMilliseconds(500)) { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever });
            content.Children.Add(dot);
            content.Children.Add(Text("Fixing…", "#F9FAFB"));
        });
        Present(anchor, TimeSpan.FromSeconds(20));
    });

    private void Fill(Action<StackPanel> build)
    {
        EnsureWindow();
        _content.Children.Clear();
        build(_content);
    }

    private void Present(Win32.RECT? anchor, TimeSpan duration)
    {
        var window = _window!;
        window.BeginAnimation(UIElement.OpacityProperty, null);
        window.Opacity = 1;
        if (!window.IsVisible) window.Show();
        window.UpdateLayout();
        Place(window, anchor);
        _hideTimer.Stop();
        _hideTimer.Interval = duration;
        _hideTimer.Start();
    }

    private void Hide()
    {
        _hideTimer.Stop();
        _working = false;
        if (_window is not { IsVisible: true } window) return;
        var fade = new DoubleAnimation(0, TimeSpan.FromMilliseconds(180));
        fade.Completed += (_, _) =>
        {
            if (window.Opacity == 0) window.Hide();
        };
        window.BeginAnimation(UIElement.OpacityProperty, fade);
    }

    /// <summary>Places the note just below the caret, kept inside the monitor's work area. Uses physical pixels.</summary>
    private static void Place(Window window, Win32.RECT? anchor)
    {
        var handle = new WindowInteropHelper(window).Handle;
        var source = PresentationSource.FromVisual(window);
        double scale = source?.CompositionTarget?.TransformToDevice.M11 ?? 1.0;
        int width = (int)Math.Ceiling(window.ActualWidth * scale);
        int height = (int)Math.Ceiling(window.ActualHeight * scale);

        Win32.POINT point;
        if (anchor is { } caret)
        {
            point = new Win32.POINT { X = caret.Left - 12, Y = caret.Bottom + (int)(8 * scale) };
        }
        else
        {
            Win32.GetCursorPos(out point);
            point.Y += (int)(24 * scale);
        }

        var info = new Win32.MONITORINFO { cbSize = System.Runtime.InteropServices.Marshal.SizeOf<Win32.MONITORINFO>() };
        Win32.GetMonitorInfo(Win32.MonitorFromPoint(point, 2), ref info);
        var work = info.rcWork;
        int x = Math.Clamp(point.X, work.Left + 8, Math.Max(work.Left + 8, work.Right - width - 8));
        int y = point.Y;
        // Flip above the caret when there is no room below.
        if (y + height > work.Bottom - 8 && anchor is { } above) y = above.Top - height - (int)(8 * scale);
        y = Math.Clamp(y, work.Top + 8, Math.Max(work.Top + 8, work.Bottom - height - 8));
        const uint NoSizeNoActivate = 0x0001 | 0x0010;
        Win32.SetWindowPos(handle, new IntPtr(-1), x, y, 0, 0, NoSizeNoActivate);
    }

    private void EnsureWindow()
    {
        if (_window is not null) return;
        _content = new StackPanel { Orientation = Orientation.Horizontal };
        _pill = new Border
        {
            Background = Brush("#1F2329"),
            BorderBrush = Brush("#3A3F49"),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(12, 7, 14, 7),
            Margin = new Thickness(10),
            Child = _content,
            Effect = new System.Windows.Media.Effects.DropShadowEffect
            {
                BlurRadius = 16,
                ShadowDepth = 3,
                Opacity = 0.28,
                Color = Colors.Black,
            },
        };
        _window = new Window
        {
            WindowStyle = WindowStyle.None,
            AllowsTransparency = true,
            Background = Brushes.Transparent,
            ShowInTaskbar = false,
            ShowActivated = false,
            Topmost = true,
            Focusable = false,
            IsHitTestVisible = false,
            SizeToContent = SizeToContent.WidthAndHeight,
            Content = _pill,
            Left = -10000,
            Top = -10000,
            FontFamily = new FontFamily("Segoe UI Variable Text, Segoe UI"),
        };
        _window.SourceInitialized += (_, _) =>
        {
            var handle = new WindowInteropHelper(_window).Handle;
            long style = Win32.GetWindowLongPtr(handle, Win32.GWL_EXSTYLE).ToInt64();
            style |= Win32.WS_EX_NOACTIVATE | Win32.WS_EX_TOOLWINDOW | Win32.WS_EX_TRANSPARENT;
            Win32.SetWindowLongPtr(handle, Win32.GWL_EXSTYLE, new IntPtr(style));
        };
    }

    private static TextBlock Icon(string glyph, string color) => new()
    {
        Text = glyph,
        Foreground = Brush(color),
        FontWeight = FontWeights.Bold,
        FontSize = 13,
        Margin = new Thickness(0, 0, 8, 0),
        VerticalAlignment = VerticalAlignment.Center,
    };

    private static TextBlock Text(string text, string color, bool strike = false, bool bold = false, bool small = false) => new()
    {
        Text = text,
        Foreground = Brush(color),
        FontSize = small ? 11.5 : 13,
        FontWeight = bold ? FontWeights.SemiBold : FontWeights.Normal,
        TextDecorations = strike ? TextDecorations.Strikethrough : null,
        Margin = new Thickness(small ? 10 : 0, 0, 6, 0),
        VerticalAlignment = VerticalAlignment.Center,
        MaxWidth = 420,
        TextTrimming = TextTrimming.CharacterEllipsis,
    };

    private static string Shorten(string text) => text.Length <= 40 ? text : text[..37] + "…";

    private static SolidColorBrush Brush(string color)
    {
        var brush = new SolidColorBrush((Color)ColorConverter.ConvertFromString(color));
        brush.Freeze();
        return brush;
    }
}
