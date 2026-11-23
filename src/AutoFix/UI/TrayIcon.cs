using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;
using AutoFix.Services;

namespace AutoFix.UI;

/// <summary>Tray icon with quick controls: pause, turn off for the current app, settings, quit.</summary>
internal sealed class TrayIcon : IDisposable
{
    private readonly AutoFixService _service;
    private readonly NotifyIcon _icon;
    private readonly Icon _normal;
    private readonly Icon _paused;
    private readonly ToolStripMenuItem _status = new() { Enabled = false };
    private readonly ToolStripMenuItem _pause = new("Pause AutoFix");
    private readonly ToolStripMenuItem _appToggle = new();
    private string? _appName;

    public TrayIcon(AutoFixService service, Action openSettings, Action quit)
    {
        _service = service;
        using (var stream = System.Windows.Application.GetResourceStream(new Uri("pack://application:,,,/Assets/autofix-logo.ico")).Stream)
        {
            _normal = new Icon(stream, SystemInformation.SmallIconSize);
        }
        _paused = Grayscale(_normal);

        _pause.Click += (_, _) => _service.Update(s => s.Enabled = !s.Enabled);
        _appToggle.Click += (_, _) => ToggleApp();
        var menu = new ContextMenuStrip();
        menu.Items.Add(_status);
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(_pause);
        menu.Items.Add(_appToggle);
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Settings", null, (_, _) => openSettings());
        menu.Items.Add("Quit AutoFix", null, (_, _) => quit());
        menu.Opening += (_, _) => RefreshMenu();

        _icon = new NotifyIcon { Icon = _normal, ContextMenuStrip = menu, Visible = true };
        _icon.MouseClick += (_, e) =>
        {
            if (e.Button == MouseButtons.Left) openSettings();
        };
        _service.Changed += OnChanged;
        Refresh();
    }

    public void ShowBalloon(string title, string text) => _icon.ShowBalloonTip(4000, title, text, ToolTipIcon.None);

    // Settings can change on any thread; the tray belongs to the UI thread.
    private void OnChanged() => System.Windows.Application.Current?.Dispatcher.BeginInvoke(Refresh);

    private void Refresh()
    {
        var settings = _service.Settings;
        bool active = settings.Enabled && _service.IsReady;
        _icon.Icon = active ? _normal : _paused;
        // Tooltips are limited to 63 characters.
        string tip = StatusLine();
        _icon.Text = tip.Length > 63 ? tip[..60] + "…" : tip;
    }

    private string StatusLine()
    {
        var settings = _service.Settings;
        if (!settings.Enabled) return "AutoFix is paused";
        if (!_service.IsReady) return "AutoFix needs an API key";
        if (_service.LastError is { } error) return "AutoFix: " + error;
        return "AutoFix is on";
    }

    private void RefreshMenu()
    {
        var settings = _service.Settings;
        _status.Text = StatusLine();
        _pause.Checked = !settings.Enabled;
        _appName = _service.LastApp?.ProcessName;
        _appToggle.Visible = !string.IsNullOrEmpty(_appName);
        if (_appName is null) return;
        bool blocked = settings.IsBlocked(_appName);
        _appToggle.Text = $"Off in {Path.GetFileNameWithoutExtension(_appName)}";
        _appToggle.Checked = blocked;
    }

    private void ToggleApp()
    {
        if (_appName is not { } app) return;
        _service.Update(s =>
        {
            if (s.IsBlocked(app)) s.BlockedApps.RemoveAll(a => string.Equals(a, app, StringComparison.OrdinalIgnoreCase));
            else s.BlockedApps.Add(app);
        });
    }

    private static Icon Grayscale(Icon source)
    {
        using var color = source.ToBitmap();
        var gray = new Bitmap(color.Width, color.Height, PixelFormat.Format32bppArgb);
        for (int y = 0; y < color.Height; y++)
        {
            for (int x = 0; x < color.Width; x++)
            {
                var c = color.GetPixel(x, y);
                int l = (int)(c.R * 0.3 + c.G * 0.59 + c.B * 0.11);
                gray.SetPixel(x, y, Color.FromArgb(c.A * 6 / 10, l, l, l));
            }
        }
        return Icon.FromHandle(gray.GetHicon());
    }

    public void Dispose()
    {
        _service.Changed -= OnChanged;
        _icon.Visible = false;
        _icon.Dispose();
        _normal.Dispose();
        _paused.Dispose();
    }
}
