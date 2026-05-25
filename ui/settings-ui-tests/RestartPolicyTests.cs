using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class RestartPolicyTests
{
    [TestMethod]
    public void AttemptsRollOutOfWindow()
    {
        var clock = new FakeTimeProvider();
        var policy = new RestartPolicy(maxAttempts: 3, window: TimeSpan.FromSeconds(30), timeProvider: clock);

        Assert.IsTrue(policy.TryRecordAttempt());
        Assert.IsTrue(policy.TryRecordAttempt());
        Assert.IsTrue(policy.TryRecordAttempt());
        Assert.IsFalse(policy.TryRecordAttempt());

        clock.Advance(TimeSpan.FromSeconds(31));

        Assert.IsTrue(policy.TryRecordAttempt());
    }

    private sealed class FakeTimeProvider : TimeProvider
    {
        private DateTimeOffset now = DateTimeOffset.Parse("2026-05-25T00:00:00Z");

        public override DateTimeOffset GetUtcNow() => now;

        public void Advance(TimeSpan value) => now += value;
    }
}
