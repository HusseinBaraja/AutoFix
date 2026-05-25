using System.Windows;
using Forms = System.Windows.Forms;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class ShellTray : IDisposable
{
    private readonly Forms.NotifyIcon notifyIcon;
    private readonly Action showShell;
    private readonly Action exitShell;
    private bool disposed;

    public ShellTray(Action showShell, Action exitShell)
    {
        this.showShell = showShell;
        this.exitShell = exitShell;
        notifyIcon = new Forms.NotifyIcon
        {
            Text = "AutoFix",
            Icon = System.Drawing.Icon.ExtractAssociatedIcon(Environment.ProcessPath ?? System.Reflection.Assembly.GetExecutingAssembly().Location)
                ?? System.Drawing.SystemIcons.Application,
            Visible = true,
            ContextMenuStrip = BuildMenu(),
        };
        notifyIcon.DoubleClick += (_, _) => this.showShell();
    }

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
        notifyIcon.Visible = false;
        notifyIcon.Dispose();
    }
}
