using System.Windows;
using System.Windows.Threading;
using AutoFix.Core.Settings;
using AutoFix.Services;
using AutoFix.UI;

namespace AutoFix;

public static class Program
{
    private const string InstanceName = @"Local\AutoFix.v2.Instance";
    private const string ShowSettingsEvent = @"Local\AutoFix.v2.ShowSettings";

    [STAThread]
    public static int Main(string[] args)
    {
        // Tests run a separate instance with its own data folder.
        string? dataOverride = Environment.GetEnvironmentVariable("AUTOFIX_DATA_DIR");
        string dataDir = dataOverride ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "AutoFix");
        string suffix = dataOverride is null ? "" : "." + Convert.ToHexString(
            System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(dataDir.ToLowerInvariant())))[..12];

        using var instance = new Mutex(true, InstanceName + suffix, out bool first);
        using var showSettings = new EventWaitHandle(false, EventResetMode.AutoReset, ShowSettingsEvent + suffix);
        if (!first)
        {
            showSettings.Set();
            return 0;
        }

        Log.Initialize(dataDir);
        Native.Win32.UsePerMonitorDpi();
        var app = new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
        app.Resources.MergedDictionaries.Add(new ResourceDictionary
        {
            Source = new Uri("pack://application:,,,/AutoFix;component/UI/Styles.xaml"),
        });
        Theme.Apply(app.Resources);
        app.DispatcherUnhandledException += (_, e) =>
        {
            Log.Error("Unhandled UI error", e.Exception);
            e.Handled = true;
        };
        AppDomain.CurrentDomain.UnhandledException += (_, e) => Log.Error("Unhandled error", e.ExceptionObject as Exception);
        TaskScheduler.UnobservedTaskException += (_, e) =>
        {
            Log.Error("Unobserved task error", e.Exception);
            e.SetObserved();
        };

        var popup = new FixPopup(app.Dispatcher);
        using var service = new AutoFixService(new SettingsStore(Path.Combine(dataDir, "settings.json")), popup,
            manageStartup: dataOverride is null);
        try
        {
            service.Start();
        }
        catch (InvalidOperationException ex)
        {
            Log.Error("Startup failed", ex);
            MessageBox.Show("AutoFix could not watch the keyboard: " + ex.InnerException?.Message, "AutoFix",
                MessageBoxButton.OK, MessageBoxImage.Error);
            return 1;
        }

        SettingsWindow? window = null;
        bool hintShown = false;
        TrayIcon? tray = null;

        void OpenSettings()
        {
            if (window is null)
            {
                window = new SettingsWindow(service);
                window.Closed += (_, _) =>
                {
                    window = null;
                    if (!hintShown)
                    {
                        hintShown = true;
                        tray?.ShowBalloon("AutoFix is still running",
                            service.IsReady ? "It fixes your typing in every app. Open settings from the tray icon." : "Add an API key from the tray icon to start.");
                    }
                };
                window.Show();
            }
            if (window.WindowState == WindowState.Minimized) window.WindowState = WindowState.Normal;
            window.Activate();
        }

        tray = new TrayIcon(service, OpenSettings, () => app.Shutdown());

        // A second launch asks this instance to show its settings.
        var listener = new Thread(() =>
        {
            while (showSettings.WaitOne()) app.Dispatcher.BeginInvoke(OpenSettings);
        }) { IsBackground = true };
        listener.Start();

        bool background = args.Contains("--background", StringComparer.OrdinalIgnoreCase);
        if (!background || !service.IsReady) app.Dispatcher.BeginInvoke(OpenSettings, DispatcherPriority.Loaded);

        app.Run();
        tray.Dispose();
        Log.Info("Stopped.");
        return 0;
    }
}
