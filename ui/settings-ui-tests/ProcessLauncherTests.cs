using AutoFix.SettingsUi;
using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ProcessLauncherTests
{
    [TestMethod]
    public void EngineProcessLaunchesCurrentHostAsEngineRole()
    {
        var startInfo = EngineProcessLauncher.CreateStartInfo(@"C:\AutoFix\Autofix.exe");

        Assert.AreEqual(@"C:\AutoFix\Autofix.exe", startInfo.FileName);
        Assert.AreEqual(Program.EngineArgument, startInfo.Arguments);
        Assert.IsFalse(startInfo.UseShellExecute);
        Assert.IsTrue(startInfo.CreateNoWindow);
        Assert.AreEqual(@"C:\AutoFix", startInfo.WorkingDirectory);
    }

    [TestMethod]
    public void ShutdownHelperLaunchesCurrentHostAsShutdownRole()
    {
        var startInfo = ShutdownHelperLauncher.CreateStartInfo(@"C:\AutoFix\Autofix.exe");

        Assert.AreEqual(@"C:\AutoFix\Autofix.exe", startInfo.FileName);
        Assert.AreEqual(Program.ShutdownAllArgument, startInfo.Arguments);
        Assert.IsFalse(startInfo.UseShellExecute);
        Assert.IsTrue(startInfo.CreateNoWindow);
    }
}
