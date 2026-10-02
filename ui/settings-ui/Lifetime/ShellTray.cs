using System.Drawing;
using System.Runtime.InteropServices;
using AutoFix.SettingsUi.Ipc;
using Forms = System.Windows.Forms;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class ShellTray : IDisposable
{
    private readonly Forms.NotifyIcon notifyIcon;
    private readonly Action showShell;
    private readonly Action exitShell;
    private bool disposed;
    private readonly Forms.Timer statusTimer;
    private readonly BackgroundIpcClient ipc = new();
    private readonly Dictionary<string, Icon> icons = new();
    private bool polling;
    private bool trayStateEnabled = true;

    public ShellTray(Action showShell, Action exitShell)
    {
        this.showShell = showShell;
        this.exitShell = exitShell;
        notifyIcon = new Forms.NotifyIcon
        {
            Text = "AutoFix",
            Icon = System.Drawing.Icon.ExtractAssociatedIcon(Environment.ProcessPath ?? System.Reflection.Assembly.GetExecutingAssembly().Location)
                ?? (Icon)System.Drawing.SystemIcons.Application.Clone(),
            Visible = true,
            ContextMenuStrip = BuildMenu(),
        };
        notifyIcon.DoubleClick += (_, _) => this.showShell();
        var originalIcon = notifyIcon.Icon!;
        BuildStateIcons(originalIcon);
        SetState("idle");
        originalIcon.Dispose();
        statusTimer = new Forms.Timer { Interval = 250 };
        statusTimer.Tick += async (_, _) => await RefreshStateAsync();
        statusTimer.Start();
    }

    internal static string NormalizeState(string? state) => state is "idle" or "active" or "correcting" or "blocked" or "error" ? state : "idle";

    internal bool IsVisible => notifyIcon.Visible;
    internal string StatusText => notifyIcon.Text;

    internal void SetState(string state)
    {
        if (disposed) return;
        state = trayStateEnabled ? NormalizeState(state) : "idle";
        notifyIcon.Text = $"AutoFix — {state}";
        notifyIcon.Icon = icons[state];
    }

    internal void UpdateStatus(AppStatusResponse? status)
    {
        if (status is not null) trayStateEnabled = status.TrayStateEnabled;
        SetState(status?.TrayState ?? "error");
    }

    private async Task RefreshStateAsync()
    {
        if (disposed || polling) return;
        polling = true;
        try
        {
            var result = await ipc.GetStatusAsync();
            UpdateStatus(result.Value);
        }
        catch (Exception error) when (error is System.IO.IOException or TimeoutException or OperationCanceledException or System.Text.Json.JsonException)
        {
            SetState("error");
        }
        finally { polling = false; }
    }

    private void BuildStateIcons(Icon original)
    {
        icons["idle"] = (Icon)original.Clone();
        foreach (var (state, color) in new[] { ("active", Color.SeaGreen), ("correcting", Color.DodgerBlue), ("blocked", Color.DarkOrange), ("error", Color.Firebrick) })
        {
            using var bitmap = new Bitmap(32, 32);
            using (var graphics = Graphics.FromImage(bitmap))
            {
                graphics.DrawIcon(original, new Rectangle(0, 0, 32, 32));
                using var brush = new SolidBrush(color);
                graphics.FillEllipse(Brushes.White, 19, 19, 13, 13);
                graphics.FillEllipse(brush, 21, 21, 9, 9);
            }
            var handle = bitmap.GetHicon();
            try { icons[state] = (Icon)Icon.FromHandle(handle).Clone(); }
            finally { DestroyIcon(handle); }
        }
    }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DestroyIcon(IntPtr icon);

    private Forms.ContextMenuStrip BuildMenu()
    {
        var menu = new Forms.ContextMenuStrip();
        menu.Items.Add("Open settings", null, (_, _) => showShell());
        menu.Items.Add("Exit", null, (_, _) => exitShell());
        return menu;
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        statusTimer.Stop();
        statusTimer.Dispose();
        notifyIcon.Visible = false;
        notifyIcon.Dispose();
        foreach (var icon in icons.Values) icon.Dispose();
    }
}
