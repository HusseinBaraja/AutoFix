using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi;

internal interface IAppRoleHost
{
    void SetAppIdentity();
    int RunSettings();
    int RunEngine();
    void WriteUsage();
}

internal static class Program
{
    internal const string EngineArgument = "--engine";

    [STAThread]
    public static int Main(string[] args) => Run(args, new AppRoleHost());

    internal static int Run(IReadOnlyList<string> args, IAppRoleHost host)
    {
        if (args.Count == 0)
        {
            host.SetAppIdentity();
            return host.RunSettings();
        }

        if (args.Count == 1 && args[0] == EngineArgument)
        {
            host.SetAppIdentity();
            return host.RunEngine();
        }

        host.WriteUsage();
        return 2;
    }
}

internal sealed class AppRoleHost : IAppRoleHost
{
    public void SetAppIdentity() => AppIdentity.SetCurrentProcess();

    public int RunSettings()
    {
        var app = new App();
        app.InitializeComponent();
        app.Run();
        return 0;
    }

    public int RunEngine()
    {
        return NativeEngine.RunBackground();
    }

    public void WriteUsage() => Console.Error.WriteLine("usage: Autofix.exe [--engine]");
}
