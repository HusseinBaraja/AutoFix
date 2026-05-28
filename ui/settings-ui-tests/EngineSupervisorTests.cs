using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class EngineSupervisorTests
{
    [TestMethod]
    public void IntentionalStopDoesNotRestartEngine()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher);
        var classifications = new List<EngineExitClassification>();
        supervisor.EngineExitClassified += (_, classification) => classifications.Add(classification);

        supervisor.Start();
        supervisor.StopIntentionally(ShutdownReason.UserExit);

        CollectionAssert.AreEqual(new[] { EngineExitClassification.Intentional }, classifications);
        Assert.AreEqual(1, launcher.Starts);
    }

    [TestMethod]
    public void UnexpectedDeathRestartsUntilPolicyAllows()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher, new RestartPolicy(maxAttempts: 3));
        var classifications = new List<EngineExitClassification>();
        supervisor.EngineExitClassified += (_, classification) => classifications.Add(classification);

        supervisor.Start();
        launcher.LastProcess!.Exit(99);

        CollectionAssert.Contains(classifications, EngineExitClassification.Restarted);
        Assert.AreEqual(2, launcher.Starts);
    }

    [TestMethod]
    public void RestartPolicyStopsAfterThreeAttemptsInsideWindow()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher, new RestartPolicy(maxAttempts: 3, window: TimeSpan.FromSeconds(30)));
        var classifications = new List<EngineExitClassification>();
        supervisor.EngineExitClassified += (_, classification) => classifications.Add(classification);

        supervisor.Start();
        launcher.LastProcess!.Exit(99);
        launcher.LastProcess!.Exit(99);
        launcher.LastProcess!.Exit(99);
        launcher.LastProcess!.Exit(99);

        Assert.AreEqual(4, launcher.Starts);
        Assert.AreEqual(3, classifications.Count(item => item == EngineExitClassification.Restarted));
        Assert.AreEqual(1, classifications.Count(item => item == EngineExitClassification.RestartExhausted));
    }

    [TestMethod]
    public void ExternalKillClassificationDoesNotRestart()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher);
        var classifications = new List<EngineExitClassification>();
        supervisor.EngineExitClassified += (_, classification) => classifications.Add(classification);

        supervisor.Start();
        launcher.LastProcess!.Exit(1);

        CollectionAssert.AreEqual(new[] { EngineExitClassification.ExternalKill }, classifications);
        Assert.AreEqual(1, launcher.Starts);
    }

    [TestMethod]
    public void NativeStartFailureExitCodeClassifiesAsStartFailed()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher);
        var classifications = new List<EngineExitClassification>();
        supervisor.EngineExitClassified += (_, classification) => classifications.Add(classification);

        supervisor.Start();
        launcher.LastProcess!.Exit(NativeEngine.NativeStartFailedExitCode);

        CollectionAssert.AreEqual(new[] { EngineExitClassification.StartFailed }, classifications);
        Assert.AreEqual(1, launcher.Starts);
    }

    [TestMethod]
    public void ConcurrentStartCallsCreateOneEngine()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher);

        Parallel.For(0, 32, _ => supervisor.Start());

        Assert.AreEqual(1, launcher.Starts);
    }

    [TestMethod]
    public void ConcurrentConsumesClearReasonOnce()
    {
        var intentionalStop = new IntentionalEngineStop();
        intentionalStop.Mark(ShutdownReason.UserExit);

        var matches = 0;

        Parallel.For(
            0,
            32,
            _ =>
            {
                if (intentionalStop.ConsumeIfMatches(ShutdownReason.UserExit))
                {
                    Interlocked.Increment(ref matches);
                }
            });

        Assert.AreEqual(1, matches);
        Assert.IsFalse(intentionalStop.HasCurrentStop);
    }

    [TestMethod]
    public void StaleEngineExitDoesNotRestartCurrentEngine()
    {
        var launcher = new FakeLauncher();
        var supervisor = new EngineSupervisor(launcher, new RestartPolicy(maxAttempts: 3));
        var classifications = new List<EngineExitClassification>();
        supervisor.EngineExitClassified += (_, classification) => classifications.Add(classification);

        supervisor.Start();
        var firstProcess = launcher.LastProcess!;
        supervisor.StopIntentionally(ShutdownReason.UserExit);
        supervisor.Start();

        firstProcess.RaiseExitedAgain();

        Assert.AreEqual(2, launcher.Starts);
        CollectionAssert.AreEqual(new[] { EngineExitClassification.Intentional }, classifications);
    }

    private sealed class FakeLauncher : IEngineProcessLauncher
    {
        private int starts;

        public int Starts => starts;
        public FakeProcess? LastProcess { get; private set; }

        public IEngineProcess Start()
        {
            var currentStart = Interlocked.Increment(ref starts);
            LastProcess = new FakeProcess(currentStart);
            return LastProcess;
        }
    }

    private sealed class FakeProcess(int id) : IEngineProcess
    {
        public event EventHandler? Exited;
        public int Id { get; } = id;
        public int? ExitCode { get; private set; }
        public bool HasExited { get; private set; }

        public void Exit(int exitCode)
        {
            HasExited = true;
            ExitCode = exitCode;
            Exited?.Invoke(this, EventArgs.Empty);
        }

        public void RaiseExitedAgain() => Exited?.Invoke(this, EventArgs.Empty);

        public void Kill() => Exit(0);
        public void Dispose() { }
    }
}
