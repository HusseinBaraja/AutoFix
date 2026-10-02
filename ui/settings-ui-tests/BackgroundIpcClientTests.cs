using System.IO.Pipes;
using System.Text;
using System.Text.Json;
using AutoFix.SettingsUi.Ipc;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class BackgroundIpcClientTests
{
    /// <summary>Verify current-user transport accepts new and legacy status payloads in both read modes.</summary>
    [TestMethod]
    public async Task StatusPollingWorksWithCurrentUserServerAndLegacyPayloads()
    {
        foreach (var mode in new[] { PipeTransmissionMode.Byte, PipeTransmissionMode.Message })
        foreach (var legacy in new[] { false, true })
        {
            var payload = legacy
                ? "{\"running\":true,\"correction_mode\":\"typos_only\",\"engine\":\"local\"}"
                : "{\"running\":true,\"correction_mode\":\"typos_only\",\"engine\":\"local\",\"tray_state\":\"active\",\"tray_state_enabled\":false}";
            var result = await WithResponse(mode, $"{{\"type\":\"app_status\",\"payload\":{payload}}}");
            Assert.IsTrue(result.Available);
            Assert.IsNull(result.Error);
            Assert.IsTrue(result.Value!.Running);
            Assert.AreEqual(legacy ? "idle" : "active", result.Value.TrayState);
            Assert.AreEqual(legacy, result.Value.TrayStateEnabled);
        }
    }

    /// <summary>Empty and null responses remain transport failures rather than fabricated healthy status.</summary>
    [TestMethod]
    public async Task EmptyAndNullStatusResponsesFailClosed()
    {
        await Assert.ThrowsExceptionAsync<JsonException>(() => WithResponse(PipeTransmissionMode.Message, ""));
        await Assert.ThrowsExceptionAsync<InvalidDataException>(() => WithResponse(PipeTransmissionMode.Message, "null"));
    }

    /// <summary>Serve one bounded status exchange without connecting to the user's running engine.</summary>
    private static async Task<IpcResult<AppStatusResponse>> WithResponse(PipeTransmissionMode mode, string response)
    {
        var pipeName = $"Local\\AutoFix.Status.Tests.{Guid.NewGuid():N}";
        await using var server = new NamedPipeServerStream(pipeName, PipeDirection.InOut, 1, mode,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var serving = Task.Run(async () =>
        {
            await server.WaitForConnectionAsync(deadline.Token);
            var bytes = new byte[4096];
            var count = await server.ReadAsync(bytes, deadline.Token);
            using var request = JsonDocument.Parse(bytes.AsMemory(0, count));
            Assert.AreEqual("get_app_status", request.RootElement.GetProperty("type").GetString());
            await server.WriteAsync(Encoding.UTF8.GetBytes(response), deadline.Token);
            server.WaitForPipeDrain();
            server.Disconnect();
        }, deadline.Token);
        try { return await new BackgroundIpcClient(pipeName).GetStatusAsync(); }
        finally { await serving.WaitAsync(deadline.Token); }
    }
}
