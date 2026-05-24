using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class HotkeyConflictTests
{
    [TestMethod]
    public void DuplicateHotkeysProduceConflictMessages()
    {
        var vm = CreateViewModel();
        var correct = FindHotkey(vm, "shortcuts.correct");
        var undo = FindHotkey(vm, "shortcuts.undo");

        undo.Hotkey = correct.Hotkey;
        vm.ValidateHotkeyConflicts();

        Assert.IsTrue(correct.HasHotkeyConflict);
        Assert.IsTrue(undo.HasHotkeyConflict);
        StringAssert.Contains(correct.HotkeyConflictMessage, "Undo");
        StringAssert.Contains(undo.HotkeyConflictMessage, "Correction");
    }

    [TestMethod]
    public void DistinctHotkeysHaveNoConflict()
    {
        var vm = CreateViewModel();
        var correct = FindHotkey(vm, "shortcuts.correct");
        var undo = FindHotkey(vm, "shortcuts.undo");

        correct.Hotkey = "Ctrl+Alt+Space";
        undo.Hotkey = "Ctrl+Alt+Z";
        vm.ValidateHotkeyConflicts();

        Assert.IsFalse(correct.HasHotkeyConflict);
        Assert.IsFalse(undo.HasHotkeyConflict);
        Assert.AreEqual("", correct.HotkeyConflictMessage);
        Assert.AreEqual("", undo.HotkeyConflictMessage);
    }

    [TestMethod]
    public void ClearingConflictingHotkeyClearsConflict()
    {
        var vm = CreateViewModel();
        var correct = FindHotkey(vm, "shortcuts.correct");
        var undo = FindHotkey(vm, "shortcuts.undo");

        undo.Hotkey = correct.Hotkey;
        vm.ValidateHotkeyConflicts();
        Assert.IsTrue(correct.HasHotkeyConflict);

        undo.Hotkey = "Ctrl+Shift+Z";
        vm.ValidateHotkeyConflicts();

        Assert.IsFalse(correct.HasHotkeyConflict);
        Assert.IsFalse(undo.HasHotkeyConflict);
    }

    [TestMethod]
    public void NonHotkeySettingsNeverGetConflictMessages()
    {
        var vm = CreateViewModel();
        vm.ValidateHotkeyConflicts();

        var nonHotkeys = vm.Sections
            .SelectMany(section => section.Settings)
            .Where(setting => !setting.IsHotkey);

        foreach (var setting in nonHotkeys)
        {
            Assert.IsFalse(setting.HasHotkeyConflict, $"{setting.Path} should not have conflict");
        }
    }

    [TestMethod]
    public void EmptyHotkeyDoesNotConflict()
    {
        var vm = CreateViewModel();
        var correct = FindHotkey(vm, "shortcuts.correct");
        var undo = FindHotkey(vm, "shortcuts.undo");

        correct.Hotkey = "";
        undo.Hotkey = "";
        vm.ValidateHotkeyConflicts();

        Assert.IsFalse(correct.HasHotkeyConflict);
        Assert.IsFalse(undo.HasHotkeyConflict);
    }

    private static MainWindowViewModel CreateViewModel()
    {
        using var fixture = TempConfigFixture.Create();
        return new MainWindowViewModel(
            new FakeIpcClient(),
            fixture.Storage,
            new FakeFileDialog());
    }

    private static SettingCardViewModel FindHotkey(MainWindowViewModel vm, string path) =>
        vm.Sections
            .SelectMany(section => section.Settings)
            .Single(setting => setting.Path == path);

    private sealed class FakeIpcClient : IBackgroundIpcClient
    {
        public Task<IpcResult<AppStatusResponse>> GetStatusAsync() =>
            Task.FromResult(IpcResult<AppStatusResponse>.Ok(new(true, "typos_only", "local")));

        public Task<IpcResult<CorrectionModeResponse>> GetCorrectionModeAsync() =>
            Task.FromResult(IpcResult<CorrectionModeResponse>.Ok(new("typos_only")));

        public Task<IpcResult<CorrectionEngineResponse>> GetCurrentEngineAsync() =>
            Task.FromResult(IpcResult<CorrectionEngineResponse>.Ok(new("local")));

        public Task<IpcResult<AppStatusResponse>> ReloadConfigAsync() => GetStatusAsync();

        public Task<IpcResult<SettingUpdatedResponse>> UpdateSettingAsync(string path, string value) =>
            Task.FromResult(IpcResult<SettingUpdatedResponse>.Ok(new(path)));

        public Task<IpcResult<AppRulesResponse>> ListAppRulesAsync() =>
            Task.FromResult(IpcResult<AppRulesResponse>.Ok(new(new List<AppRuleDto>())));

        public Task<IpcResult<AppRuleUpdatedResponse>> UpsertAppRuleAsync(AppRuleDto rule) =>
            Task.FromResult(IpcResult<AppRuleUpdatedResponse>.Ok(new(rule.ProcessName, rule.WindowTitlePattern)));

        public Task<IpcResult<AppRuleDeletedResponse>> DeleteAppRuleAsync(string processName, string? windowTitlePattern) =>
            Task.FromResult(IpcResult<AppRuleDeletedResponse>.Ok(new(true)));

        public Task<IpcResult<AppRulesResponse>> ResetAppRulesAsync() =>
            Task.FromResult(IpcResult<AppRulesResponse>.Ok(new(new List<AppRuleDto>())));

        public Task<IpcResult<LogsResponse>> OpenLogsAsync() =>
            Task.FromResult(IpcResult<LogsResponse>.Ok(new("", true)));

        public Task<IpcResult<CommandAcceptedResponse>> RequestUndoLastCorrectionAsync() =>
            Task.FromResult(IpcResult<CommandAcceptedResponse>.Ok(new(true, "")));

        public Task<IpcResult<CommandAcceptedResponse>> TestCorrectionEngineLaterAsync() =>
            Task.FromResult(IpcResult<CommandAcceptedResponse>.Ok(new(true, "")));

        public Task<IpcResult<BackgroundRunningResponse>> IsBackgroundRunningAsync() =>
            Task.FromResult(IpcResult<BackgroundRunningResponse>.Ok(new(true)));

        public Task<IpcResult<ShutdownAcceptedResponse>> ShutdownAllAsync() =>
            Task.FromResult(IpcResult<ShutdownAcceptedResponse>.Ok(new(true)));
    }

    private sealed class FakeFileDialog : IConfigFileDialog
    {
        public string? PickImportPath() => null;
        public string? PickExportPath() => null;
    }
}
