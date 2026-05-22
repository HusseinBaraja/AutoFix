using AutoFix.SettingsUi.Commands;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class RelayCommandTests
{
    [TestMethod]
    public void ConstructorRejectsNullExecute()
    {
        Assert.ThrowsException<ArgumentNullException>(() => new RelayCommand(null!));
    }
}
