using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Lifetime;
using System.Text.Json;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class TrayFeedbackTests
{
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
