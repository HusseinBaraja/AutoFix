using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class WindowsStartupRegistrationTests
{
    [TestMethod]
    public void ShellCommandStartsRootShell()
    {
        var command = WindowsStartupRegistration.ShellCommand();

        StringAssert.EndsWith(command, "Autofix.exe\"");
        Assert.IsFalse(command.Contains("--background", StringComparison.Ordinal));
    }
}
