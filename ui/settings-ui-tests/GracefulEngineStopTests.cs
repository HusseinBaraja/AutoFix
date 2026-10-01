using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class GracefulEngineStopTests
{
    [TestMethod]
    public void IntentionalStopSignalsAndWaitsWithoutKilling()
    {
        var calls = new List<string>();
        GracefulEngineStop.Run(
            () => false,
            () => { calls.Add("signal"); return true; },
            timeout => { Assert.IsTrue(timeout > 0); calls.Add("drain"); return true; },
            () => calls.Add("kill"));
        CollectionAssert.AreEqual(new[] { "signal", "drain" }, calls);
    }

    [TestMethod]
    public void TimedOutCleanupKillsOnlyAfterWaiting()
    {
        var calls = new List<string>();
        GracefulEngineStop.Run(
            () => false,
            () => { calls.Add("signal"); return true; },
            _ => { calls.Add("drain"); return false; },
            () => calls.Add("kill"));
        CollectionAssert.AreEqual(new[] { "signal", "drain", "kill" }, calls);
    }

    [TestMethod]
    public void ExitedEngineNeedsNoSignalOrKill()
    {
        GracefulEngineStop.Run(() => true,
            () => throw new AssertFailedException("signal"),
            _ => throw new AssertFailedException("wait"),
            () => throw new AssertFailedException("kill"));
    }

    [TestMethod]
    public void StopSignalUsesTheEngineProcessId()
    {
        var processId = int.MaxValue;
        using var signal = new EventWaitHandle(false, EventResetMode.ManualReset,
            $@"Local\AutoFix.EngineStop.{processId}");
        Assert.IsFalse(signal.WaitOne(0));
        Assert.IsTrue(GracefulEngineStop.TrySignal(processId));
        Assert.IsTrue(signal.WaitOne(0));
        Assert.IsFalse(GracefulEngineStop.TrySignal(processId - 1));
    }
}
