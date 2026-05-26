using AutoFix.SettingsUi;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ProgramDispatchTests
{
    [TestMethod]
    public void NoArgumentsRunSettingsRoleAfterSettingIdentity()
    {
        var host = new FakeAppRoleHost(settingsExitCode: 11);

        var exitCode = Program.Run(Array.Empty<string>(), host);

        Assert.AreEqual(11, exitCode);
        CollectionAssert.AreEqual(new[] { "identity", "settings" }, host.Calls);
    }

    [TestMethod]
    public void EngineArgumentRunsEngineRoleAfterSettingIdentity()
    {
        var host = new FakeAppRoleHost(engineExitCode: 12);

        var exitCode = Program.Run(new[] { Program.EngineArgument }, host);

        Assert.AreEqual(12, exitCode);
        CollectionAssert.AreEqual(new[] { "identity", "engine" }, host.Calls);
    }

    [TestMethod]
    public void UnknownArgumentsReturnUsageErrorWithoutIdentity()
    {
        var host = new FakeAppRoleHost();

        var exitCode = Program.Run(new[] { "--background" }, host);

        Assert.AreEqual(2, exitCode);
        CollectionAssert.AreEqual(new[] { "usage" }, host.Calls);
    }

    private sealed class FakeAppRoleHost(
        int settingsExitCode = 0,
        int engineExitCode = 0) : IAppRoleHost
    {
        public List<string> Calls { get; } = new();

        public void SetAppIdentity() => Calls.Add("identity");

        public int RunSettings()
        {
            Calls.Add("settings");
            return settingsExitCode;
        }

        public int RunEngine()
        {
            Calls.Add("engine");
            return engineExitCode;
        }

        public void WriteUsage() => Calls.Add("usage");
    }
}
