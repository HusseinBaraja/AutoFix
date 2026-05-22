using AutoFix.SettingsUi.Settings;
using System.Globalization;

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

    [TestMethod]
    public void NumberParsingUsesInvariantCulture()
    {
        var originalCulture = CultureInfo.CurrentCulture;

        try
        {
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("fr-FR");

            Assert.AreEqual(1234, ConfigValue.Int("1234", "int"));
            Assert.AreEqual(1234567890123L, ConfigValue.Long("1234567890123", "long"));
            Assert.AreEqual(1234.5, ConfigValue.Double("1,234.5", "double"));
        }
        finally
        {
            CultureInfo.CurrentCulture = originalCulture;
        }
    }
}
