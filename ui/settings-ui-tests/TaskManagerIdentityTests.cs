using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class TaskManagerIdentityTests
{
    [TestMethod]
    public void WindowIdentityUsesRequestedTaskManagerLabels()
    {
        Assert.AreEqual("Autofix", AppWindowIdentity.AppDisplayName);
    }

    [TestMethod]
    public void QuoteDoublesInternalQuotesForWindowsCommandLine()
    {
        Assert.AreEqual("\"C:\\App\"\"Name\\Autofix.exe\"", AppWindowIdentity.Quote("C:\\App\"Name\\Autofix.exe"));
    }
}
