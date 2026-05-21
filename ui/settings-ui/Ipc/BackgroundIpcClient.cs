using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace AutoFix.SettingsUi.Ipc;

public sealed class BackgroundIpcClient
{
    private const string PipeName = @"Local\AutoFix.Background.Ipc";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };

    public async Task<IpcResult<AppStatusResponse>> GetStatusAsync()
    {
        var response = await SendAsync(new IpcEnvelope("get_app_status")).ConfigureAwait(false);
        return response.ReadPayload<AppStatusResponse>("app_status");
    }

    public async Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync()
    {
        var response = await SendAsync(new IpcEnvelope("get_correction_mode")).ConfigureAwait(false);
        return response.ReadPayload<CorrectionModeResponse>("correction_mode");
    }

    public async Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync()
    {
        var response = await SendAsync(new IpcEnvelope("get_current_engine")).ConfigureAwait(false);
        return response.ReadPayload<CorrectionEngineResponse>("current_engine");
    }

    public async Task<IpcResult<AppStatusResponse>> ReloadConfigAsync()
    {
        var response = await SendAsync(new IpcEnvelope("reload_config")).ConfigureAwait(false);
        return response.ReadPayload<AppStatusResponse>("config_reloaded");
    }

    public async Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value)
    {
        var payload = JsonSerializer.SerializeToElement(new { path, value }, JsonOptions);
        var response = await SendAsync(new IpcEnvelope("update_setting", payload)).ConfigureAwait(false);
        return response.ReadPayload<SettingUpdatedResponse>("setting_updated");
    }

    public async Task<IpcResult<LogsResponse>> OpenLogsAsync()
    {
        var response = await SendAsync(new IpcEnvelope("open_logs")).ConfigureAwait(false);
        return response.ReadPayload<LogsResponse>("logs");
    }

    public async Task<IpcResult<CommandAcceptedResponse>> RequestUndoLastCorrectionAsync()
    {
        var response = await SendAsync(new IpcEnvelope("request_undo_last_correction")).ConfigureAwait(false);
        return response.ReadPayload<CommandAcceptedResponse>("undo_requested");
    }

    public async Task<IpcResult<CommandAcceptedResponse>> TestCorrectionEngineLaterAsync()
    {
        var response = await SendAsync(new IpcEnvelope("test_correction_engine_later")).ConfigureAwait(false);
        return response.ReadPayload<CommandAcceptedResponse>("test_correction_engine_queued");
    }

    public async Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync()
    {
        try
        {
            var response = await SendAsync(new IpcEnvelope("is_background_running")).ConfigureAwait(false);
            return response.ReadPayload<BackgroundRunningResponse>("background_running");
        }
        catch (IOException)
        {
            return IpcResult<BackgroundRunningResponse>.Unavailable();
        }
        catch (TimeoutException)
        {
            return IpcResult<BackgroundRunningResponse>.Unavailable();
        }
        catch (JsonException)
        {
            return IpcResult<BackgroundRunningResponse>.Unavailable();
        }
        catch (InvalidDataException)
        {
            return IpcResult<BackgroundRunningResponse>.Unavailable();
        }
        catch (OperationCanceledException)
        {
            return IpcResult<BackgroundRunningResponse>.Unavailable();
        }
    }

    private static async Task<IpcEnvelope> SendAsync(IpcEnvelope request)
    {
        await using var pipe = new NamedPipeClientStream(
            ".",
            PipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous);

        using var timeout = new CancellationTokenSource(TimeSpan.FromMilliseconds(400));
        await pipe.ConnectAsync(timeout.Token).ConfigureAwait(false);

        var requestBytes = JsonSerializer.SerializeToUtf8Bytes(request, JsonOptions);
        await pipe.WriteAsync(requestBytes, timeout.Token).ConfigureAwait(false);
        await pipe.FlushAsync(timeout.Token).ConfigureAwait(false);

        using var reader = new StreamReader(pipe, Encoding.UTF8, leaveOpen: true);
        var responseText = await reader.ReadToEndAsync(timeout.Token).ConfigureAwait(false);
        return JsonSerializer.Deserialize<IpcEnvelope>(responseText, JsonOptions)
            ?? throw new InvalidDataException("IPC response was empty.");
    }
}
