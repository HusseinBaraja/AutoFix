using System.Text.Json;

namespace AutoFix.SettingsUi.Ipc;

public sealed record IpcResult<T>(bool Available, T? Value, string? Error)
{
    public static IpcResult<T> Ok(T value) => new(true, value, null);

    public static IpcResult<T> Fail(string message) => new(true, default, message);

    public static IpcResult<T> Unavailable() => new(false, default, "Background process is not running.");
}

public static class IpcEnvelopeExtensions
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static IpcResult<T> ReadPayload<T>(this IpcEnvelope envelope, string expectedType)
    {
        if (envelope.Type == "error")
        {
            var error = envelope.Payload?.Deserialize<ErrorResponse>(JsonOptions);
            return IpcResult<T>.Fail(error?.Message ?? "Background process returned an error.");
        }

        if (envelope.Type != expectedType)
        {
            return IpcResult<T>.Fail($"Unexpected IPC response: {envelope.Type}.");
        }

        if (envelope.Payload is null)
        {
            return IpcResult<T>.Fail("IPC response did not include a payload.");
        }

        var value = envelope.Payload.Value.Deserialize<T>(JsonOptions);
        return value is null ? IpcResult<T>.Fail("IPC payload was empty.") : IpcResult<T>.Ok(value);
    }
}
