using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Lifetime;
using System.Text.Json;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class TrayFeedbackTests
{
    [TestMethod]
    public void PollingContainsNonFatalFailuresAndRecoversOnNextPoll()
    {
        RunOnSta(() =>
        {
            foreach (var error in new Exception[]
            {
                new InvalidDataException("IPC response was empty."),
                new IOException(), new TimeoutException(), new OperationCanceledException(),
                new JsonException(), new InvalidOperationException(), new UnauthorizedAccessException(),
            })
            {
                var calls = 0;
                using var tray = new ShellTray(() => { }, () => { }, () =>
                    ++calls == 1
                        ? Task.FromException<IpcResult<AppStatusResponse>>(error)
                        : Task.FromResult(IpcResult<AppStatusResponse>.Ok(new(true, "typos_only", "local", "active"))));
                tray.RefreshStateAsync().GetAwaiter().GetResult();
                Assert.AreEqual("AutoFix — error", tray.StatusText, error.GetType().Name);
                Assert.IsTrue(tray.IsVisible);
                tray.RefreshStateAsync().GetAwaiter().GetResult();
                Assert.AreEqual("AutoFix — active", tray.StatusText);
                Assert.AreEqual(2, calls, "Failed polling must release the polling guard.");
            }
        });
    }

    [TestMethod]
    public void PollingPreservesDisabledStateAndStopsAfterDisposal()
    {
        RunOnSta(() =>
        {
            var calls = 0;
            using var tray = new ShellTray(() => { }, () => { }, () =>
            {
                calls++;
                throw new InvalidDataException("IPC response was empty.");
            });
            tray.UpdateStatus(new(true, "typos_only", "local", "active", false));
            tray.RefreshStateAsync().GetAwaiter().GetResult();
            Assert.AreEqual("AutoFix — idle", tray.StatusText);
            Assert.IsTrue(tray.IsVisible);
            tray.Dispose();
            tray.RefreshStateAsync().GetAwaiter().GetResult();
            Assert.AreEqual(1, calls);
        });
    }

    [TestMethod]
    public void PollingDoesNotSwallowFatalFailures()
    {
        RunOnSta(() =>
        {
            foreach (var error in new Exception[] { new OutOfMemoryException(), new StackOverflowException(), new AccessViolationException() })
            {
                using var tray = new ShellTray(() => { }, () => { }, () => Task.FromException<IpcResult<AppStatusResponse>>(error));
                try
                {
                    tray.RefreshStateAsync().GetAwaiter().GetResult();
                    Assert.Fail("Fatal polling failures must propagate.");
                }
                catch (Exception caught) when (ReferenceEquals(caught, error)) { }
            }
        });
    }

    private static void RunOnSta(Action action)
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try { action(); }
            catch (Exception error) { failure = error; }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(10)), "Tray verification timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void TrayRemainsAvailableInEveryStateAndDisposesCleanly()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try
            {
                using var tray = new ShellTray(() => { }, () => { });
                foreach (var state in new[] { "idle", "active", "correcting", "blocked", "error", "idle" })
                {
                    tray.SetState(state);
                    Assert.IsTrue(tray.IsVisible);
                    Assert.AreEqual($"AutoFix — {state}", tray.StatusText);
                }
                tray.UpdateStatus(new AppStatusResponse(true, "typos_only", "local", "correcting", false));
                tray.UpdateStatus(null);
                Assert.IsTrue(tray.IsVisible);
                Assert.AreEqual("AutoFix — idle", tray.StatusText);
                tray.Dispose();
                tray.SetState("error");
                Assert.IsFalse(tray.IsVisible);
            }
            catch (Exception error) { failure = error; }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(5)), "Tray verification timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void StatusSupportsEveryStateAndLegacyPayloads()
    {
        foreach (var state in new[] { "idle", "active", "correcting", "blocked", "error" })
        {
            var payload = JsonSerializer.SerializeToElement(new { running = true, correction_mode = "typos_only", engine = "local", tray_state = state });
            var result = new IpcEnvelope("app_status", payload).ReadPayload<AppStatusResponse>("app_status");
            Assert.AreEqual(state, result.Value!.TrayState);
            Assert.AreEqual(state, ShellTray.NormalizeState(result.Value.TrayState));
        }
        var legacy = JsonSerializer.SerializeToElement(new { running = true, correction_mode = "typos_only", engine = "local" });
        Assert.AreEqual("idle", new IpcEnvelope("app_status", legacy).ReadPayload<AppStatusResponse>("app_status").Value!.TrayState);
        Assert.IsTrue(new IpcEnvelope("app_status", legacy).ReadPayload<AppStatusResponse>("app_status").Value!.TrayStateEnabled);
        Assert.AreEqual("idle", ShellTray.NormalizeState("unknown"));
    }
}
