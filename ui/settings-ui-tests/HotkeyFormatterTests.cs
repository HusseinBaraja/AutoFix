using AutoFix.SettingsUi.Settings;
using System.Windows.Input;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class HotkeyFormatterTests
{
    [DataTestMethod]
    [DataRow("Ctrl+Ctrl+A")]
    [DataRow("Ctrl+Control+A")]
    [DataRow("Alt+Alt+A")]
    [DataRow("Shift+Shift+A")]
    [DataRow("Win+Windows+A")]
    [DataRow("Meta+Win+A")]
    public void IsValidRejectsDuplicateModifiers(string hotkey)
    {
        Assert.IsFalse(HotkeyFormatter.IsValid(hotkey));
    }

    [TestMethod]
    public void IsValidAllowsDistinctModifiers()
    {
        Assert.IsTrue(HotkeyFormatter.IsValid("Ctrl+Alt+Shift+Win+A"));
    }

    [TestMethod]
    public void FormatReturnsEmptyForUnsupportedKey()
    {
        Assert.AreEqual("", HotkeyFormatter.Format(Key.OemPlus, ModifierKeys.Control));
    }
}
