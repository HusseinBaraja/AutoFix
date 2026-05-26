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
}
