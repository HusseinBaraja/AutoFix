namespace AutoFix.SettingsUi.Ipc;

public interface IBackgroundIpcClient
{
    Task<IpcResult<AppStatusResponse>> GetStatusAsync();
    Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync();
    Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync();
    Task<IpcResult<AppStatusResponse>> ReloadConfigAsync();
    Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value);
    Task<IpcResult<AppRulesResponse>> ListAppRulesAsync();
    Task<IpcResult<AppRuleUpdatedResponse>> UpsertAppRuleAsync(AppRuleDto rule);
    Task<IpcResult<AppRuleDeletedResponse>> DeleteAppRuleAsync(string processName, string? windowTitlePattern);
    Task<IpcResult<AppRulesResponse>> ResetAppRulesAsync();
    Task<IpcResult<LogsResponse>> OpenLogsAsync();
    Task<IpcResult<CommandAcceptedResponse>> RequestUndoLastCorrectionAsync();
    Task<IpcResult<CommandAcceptedResponse>> TestCorrectionEngineLaterAsync();
    Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync();
}
