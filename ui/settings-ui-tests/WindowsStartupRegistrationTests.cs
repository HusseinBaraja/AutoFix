using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class WindowsStartupRegistrationTests
{
    [TestMethod]
    public void BackgroundCommandStartsBackgroundMode()
    {
        var command = WindowsStartupRegistration.BackgroundCommand();

        StringAssert.EndsWith(command, "\" --background");
        StringAssert.Contains(command, "background-engine.exe");
    }
}
