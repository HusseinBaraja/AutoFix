namespace AutoFix.SettingsUi.Ipc;

public interface IBackgroundIpcClient
{
    Task<IpcResult<AppStatusResponse>> GetStatusAsync();
    Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync();
    Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync();
    Task<IpcResult<AppStatusResponse>> ReloadConfigAsync();
    Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value);
    Task<IpcResult<LogsResponse>> OpenLogsAsync();
    Task<IpcResult<CommandAcceptedResponse>> RequestUndoLastCorrectionAsync();
    Task<IpcResult<CommandAcceptedResponse>> TestCorrectionEngineLaterAsync();
    Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync();
}
