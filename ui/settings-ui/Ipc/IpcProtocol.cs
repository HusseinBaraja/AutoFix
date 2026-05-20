using System.Text.Json;
using System.Text.Json.Serialization;

namespace AutoFix.SettingsUi.Ipc;

public sealed record IpcEnvelope(
    [property: JsonPropertyName("type")] string Type,
    [property: JsonPropertyName("payload")] JsonElement? Payload = null);

public sealed record AppStatusResponse(
    [property: JsonPropertyName("running")] bool Running,
    [property: JsonPropertyName("correction_mode")] string CorrectionMode,
    [property: JsonPropertyName("engine")] string Engine);

public sealed record CorrectionModeResponse(
    [property: JsonPropertyName("mode")] string Mode);

public sealed record CorrectionEngineResponse(
    [property: JsonPropertyName("engine")] string Engine);

public sealed record SettingUpdatedResponse(
    [property: JsonPropertyName("path")] string Path);

public sealed record LogsResponse(
    [property: JsonPropertyName("log_directory")] string LogDirectory,
    [property: JsonPropertyName("opened")] bool Opened);

public sealed record CommandAcceptedResponse(
    [property: JsonPropertyName("accepted")] bool Accepted,
    [property: JsonPropertyName("message")] string Message);

public sealed record BackgroundRunningResponse(
    [property: JsonPropertyName("running")] bool Running);

public sealed record ErrorResponse(
    [property: JsonPropertyName("message")] string Message);
