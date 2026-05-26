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

public sealed record AppRuleDto(
    [property: JsonPropertyName("process_name")] string ProcessName,
    [property: JsonPropertyName("window_title_pattern")] string? WindowTitlePattern,
    [property: JsonPropertyName("list_behavior")] string ListBehavior,
    [property: JsonPropertyName("manual_shortcut_allowed")] bool ManualShortcutAllowed,
    [property: JsonPropertyName("word_count_trigger_allowed")] bool WordCountTriggerAllowed,
    [property: JsonPropertyName("character_trigger_allowed")] bool CharacterTriggerAllowed,
    [property: JsonPropertyName("local_engine_allowed")] bool LocalEngineAllowed,
    [property: JsonPropertyName("api_engine_allowed")] bool ApiEngineAllowed);

public sealed record AppRulesResponse(
    [property: JsonPropertyName("rules")] IReadOnlyList<AppRuleDto> Rules);

public sealed record AppRuleUpdatedResponse(
    [property: JsonPropertyName("process_name")] string ProcessName,
    [property: JsonPropertyName("window_title_pattern")] string? WindowTitlePattern);

public sealed record AppRuleDeletedResponse(
    [property: JsonPropertyName("deleted")] bool Deleted);

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
