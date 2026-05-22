using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ConfigValueTests
{
    [TestMethod]
    public void SplitTreatsNullAsEmpty()
    {
        var values = ConfigValue.Split(null);

        Assert.AreEqual(0, values.Count);
    }
}
