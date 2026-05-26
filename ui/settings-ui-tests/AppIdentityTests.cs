using AutoFix.SettingsUi.Lifetime;
using System.Diagnostics;
using System.Reflection;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class AppIdentityTests
{
    [TestMethod]
    public void ShellUsesSharedAutofixAppUserModelId()
    {
        Assert.AreEqual("Zerone.Autofix", AppIdentity.AppUserModelId);
    }

    [TestMethod]
    public void ShellAssemblyMetadataNamesSettingsUnderAutofixProduct()
    {
        var assembly = typeof(AppIdentity).Assembly;
        var info = FileVersionInfo.GetVersionInfo(assembly.Location);

        Assert.AreEqual("AutoFix", assembly.GetCustomAttribute<AssemblyTitleAttribute>()?.Title);
        Assert.AreEqual("AutoFix", info.FileDescription);
        Assert.AreEqual("Autofix", info.ProductName);
    }
}
