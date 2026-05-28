using AutoFix.SettingsUi.Lifetime;
using System.IO;
using WpfApplication = System.Windows.Application;
using StartupEventArgs = System.Windows.StartupEventArgs;
using ExitEventArgs = System.Windows.ExitEventArgs;
using SessionEndingCancelEventArgs = System.Windows.SessionEndingCancelEventArgs;

namespace AutoFix.SettingsUi;

public partial class App : WpfApplication
{
    private ShellLifetime? lifetime;

    protected override async void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        lifetime = ShellLifetime.Create(this);
        if (!lifetime.OwnsInstance)
        {
            try
            {
                await lifetime.SignalExistingAsync();
            }
            catch (Exception error) when (error is IOException or TimeoutException or InvalidOperationException)
            {
                System.Diagnostics.Debug.WriteLine(error);
            }

            Shutdown();
            return;
        }

        lifetime.Start(new MainWindow());
    }

    protected override void OnExit(ExitEventArgs e)
    {
        lifetime?.Dispose();
        base.OnExit(e);
    }

    protected override void OnSessionEnding(SessionEndingCancelEventArgs e)
    {
        lifetime?.Shutdown(ShutdownReason.SessionEnding);
        base.OnSessionEnding(e);
    }
}
