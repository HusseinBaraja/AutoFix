using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class SingleInstanceTests
{
    [TestMethod]
    public void ExistingUnownedMutexCanBecomePrimaryInstance()
    {
        var name = UniqueMutexName();
        using var first = new Mutex(true, name, out _);
        first.ReleaseMutex();

        using var second = new Mutex(true, name, out var createdNew);
        var acquired = SingleInstance.TryAcquireExistingMutex(second);

        Assert.IsFalse(createdNew);
        Assert.IsTrue(acquired);
        second.ReleaseMutex();
    }

    [TestMethod]
    public void ExistingOwnedMutexStaysSecondaryInstance()
    {
        var name = UniqueMutexName();
        using var ready = new ManualResetEventSlim();
        using var release = new ManualResetEventSlim();
        Exception? threadError = null;
        var owner = new Thread(() =>
        {
            try
            {
                using var first = new Mutex(true, name, out _);
                ready.Set();
                release.Wait();
                first.ReleaseMutex();
            }
            catch (Exception error)
            {
                threadError = error;
                ready.Set();
            }
        });
        owner.Start();
        ready.Wait();
        if (threadError is not null)
        {
            throw threadError;
        }

        try
        {
            using var second = new Mutex(true, name, out var createdNew);
            var acquired = SingleInstance.TryAcquireExistingMutex(second);

            Assert.IsFalse(createdNew);
            Assert.IsFalse(acquired);
        }
        finally
        {
            release.Set();
            owner.Join();
        }
    }

    [TestMethod]
    public void ActivationCallbackExceptionIsContained()
    {
        var activated = false;

        var succeeded = SingleInstance.TryActivate(() =>
        {
            activated = true;
            throw new InvalidOperationException("Activation failed.");
        });

        Assert.IsTrue(activated);
        Assert.IsFalse(succeeded);
    }

    [TestMethod]
    public async Task ListenerTaskExceptionIsObserved()
    {
        var failedTask = Task.FromException(new InvalidOperationException("Listener failed."));

        await SingleInstance.ObserveListenerTaskAsync(failedTask);

        Assert.IsTrue(failedTask.IsFaulted);
    }

    [TestMethod]
    public void DisposeCanBeCalledTwice()
    {
        var name = UniqueMutexName();
        var mutex = new Mutex(true, name, out _);
        var singleInstance = new SingleInstance(mutex, ownsInstance: true, activated: () => { });

        singleInstance.Dispose();
        singleInstance.Dispose();
    }

    [TestMethod]
    public async Task SignalExistingAfterDisposeThrowsObjectDisposedException()
    {
        var name = UniqueMutexName();
        var mutex = new Mutex(true, name, out _);
        var singleInstance = new SingleInstance(mutex, ownsInstance: true, activated: () => { });
        singleInstance.Dispose();

        var exception = await Assert.ThrowsExceptionAsync<ObjectDisposedException>(
            singleInstance.SignalExistingAsync);

        Assert.AreEqual(nameof(SingleInstance), exception.ObjectName);
    }

    private static string UniqueMutexName() =>
        $"Local\\AutoFix.Tests.{Guid.NewGuid():N}";
}
