using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using AutoFix.SettingsUi.Ipc;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

public sealed partial class MainWindowViewModelTests
{
    [DataTestMethod]
    [DataRow(false)]
    [DataRow(true)]
    public async Task BundleImportWaitsForConfirmationAndReloadsAfterSaving(bool startupFails)
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = ConfigTransferStorageTests.CreateTransfer(fixture);
        var imported = AppConfig.Default(); imported.Triggers.WordCount = 26;
        var path = Path.Combine(fixture.Root, "import.zip"); transfer.Export(path, imported, false);
        var keys = new FakeApiKeyStore(); keys.Stored.Add("openai_compatible");
        var startup = new FakeStartupRegistration { FailApplication = startupFails };
        var ipc = new FakeBackgroundIpcClient();
        var storage = new AppRuleStorage(ConfigTransferStorageTests.Database(fixture));
        ipc.AppRules.AddRange(storage.List().Select(AppRuleStorage.ToDto));
        var vm = new MainWindowViewModel(ipc, fixture.Storage, storage, new TransferDialog(path, null), keys, startup, new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        var before = File.ReadAllText(fixture.Path);
        vm.ImportConfigCommand.Execute(null);
        Assert.IsTrue(vm.IsImportPreviewVisible);
        Assert.IsFalse(vm.CanEditSettings);
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual(0, ipc.ReloadCount);
        vm.CancelImportCommand.Execute(null);
        vm.ConfirmImportCommand.Execute(null);
        Assert.IsFalse(vm.IsImportPreviewVisible);
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual(0, ipc.ReloadCount);
        Assert.AreEqual(0, startup.ApplyCount);

        vm.ImportConfigCommand.Execute(null);
        ipc.ReloadUnavailable = true;
        vm.ConfirmImportCommand.Execute(null);
        await WaitForAsync(() => vm.StatusTitle == "Settings imported.");
        Assert.AreEqual(26, fixture.Storage.Load(fixture.Path).Triggers.WordCount);
        Assert.AreEqual("26", Card(vm, "triggers.word_count").TextValue);
        Assert.AreEqual(1, ipc.ReloadCount);
        Assert.AreEqual(1, startup.ApplyCount);
        Assert.IsFalse(vm.IsImportPreviewVisible);
        Assert.IsTrue(keys.Stored.Contains("openai_compatible"));
        Assert.IsNull(keys.SavedProfile);
        StringAssert.Contains(vm.StatusDetail, "Background process");
        StringAssert.Contains(vm.StatusDetail, "Settings will load on next start.");
        if (startupFails) StringAssert.Contains(vm.StatusDetail, "Windows startup registration failed");
    }

    [DataTestMethod]
    [DataRow("syntax")]
    [DataRow("timeout")]
    [DataRow("zip")]
    public async Task InvalidImportReportsErrorWithoutReloadOrSavedChanges(string fault)
    {
        using var fixture = TempConfigFixture.Create();
        var importedPath = Path.Combine(fixture.Root, fault == "zip" ? "invalid.zip" : "invalid.toml");
        var config = AppConfig.Default(); config.Api.TimeoutManualMs = 0;
        File.WriteAllText(importedPath, fault == "syntax" ? "[broken\ninvalid" : fault == "zip" ? "not a ZIP" : ConfigStorage.ToToml(config));
        var ipc = new FakeBackgroundIpcClient();
        var vm = new MainWindowViewModel(ipc, fixture.Storage, new AppRuleStorage(ConfigTransferStorageTests.Database(fixture)),
            new ImportOnlyDialog(importedPath), new FakeApiKeyStore(), new FakeStartupRegistration(), new FakeSettingsConsent());
        await vm.LoadSettingsAsync();
        var before = File.ReadAllText(fixture.Path);
        vm.ImportConfigCommand.Execute(null);
        Assert.AreEqual("Import failed.", vm.StatusTitle);
        Assert.IsFalse(vm.IsImportPreviewVisible);
        Assert.AreEqual(before, File.ReadAllText(fixture.Path));
        Assert.AreEqual(0, ipc.ReloadCount);
    }

    [TestMethod]
    public void TransferWindowPreviewsCancelsAndAppliesReviewedBundle()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                using var fixture = TempConfigFixture.Create();
                var config = AppConfig.Default(); config.Onboarding.Completed = true;
                fixture.Storage.Save(config);
                var transfer = ConfigTransferStorageTests.CreateTransfer(fixture);
                var imported = AppConfig.Default(); imported.Onboarding.Completed = true; imported.Triggers.WordCount = 28;
                var importPath = Path.Combine(fixture.Root, "import.zip"); transfer.Export(importPath, imported, false);
                var exportPath = Path.Combine(fixture.Root, "export.zip");
                new DictionaryStorage(ConfigTransferStorageTests.Database(fixture)).Save(new() { Word = "saved-pair", Replacement = "saved-replacement", Language = "und", Source = "pair" });
                var storage = new AppRuleStorage(ConfigTransferStorageTests.Database(fixture));
                var ipc = new FakeBackgroundIpcClient(); ipc.AppRules.AddRange(storage.List().Select(AppRuleStorage.ToDto));
                var vm = new MainWindowViewModel(ipc, fixture.Storage, storage, new TransferDialog(importPath, exportPath),
                    new FakeApiKeyStore(), new FakeStartupRegistration(), new FakeSettingsConsent());
                window = new MainWindow(vm) { ShowActivated = false, Left = -10000, Top = -10000, Width = 1080, Height = 720 };
                window.Show(); Pump();
                vm.SearchText = "import export"; Pump();
                Assert.AreEqual("Advanced", vm.SelectedSection?.Name);
                void Click(string label)
                {
                    var button = Descendants(window).OfType<Button>().Single(b => b.IsVisible && Equals(b.Content, label));
                    var peer = new System.Windows.Automation.Peers.ButtonAutomationPeer(button);
                    ((System.Windows.Automation.Provider.IInvokeProvider)peer.GetPattern(System.Windows.Automation.Peers.PatternInterface.Invoke)).Invoke();
                    Pump();
                }
                Click("Export…");
                Assert.AreEqual("Settings exported.", vm.StatusTitle);
                Assert.IsFalse(ConfigTransferStorageTests.Contents(exportPath).ContainsKey("learned-rules.json"));
                var checkbox = Descendants(window).OfType<CheckBox>().Single(b => b.IsVisible && Equals(b.Content, "Include learned rules in export"));
                Assert.IsFalse(checkbox.IsChecked == true);
                checkbox.SetCurrentValue(System.Windows.Controls.Primitives.ToggleButton.IsCheckedProperty, true); Pump();
                Click("Export…");
                StringAssert.Contains(ConfigTransferStorageTests.Contents(exportPath)["learned-rules.json"], "saved-pair");
                SavePreview(window, "Transfer", "export");
                var before = File.ReadAllText(fixture.Path);
                Click("Import…");
                var grid = (DataGrid)window.FindName("ImportChangesGrid");
                Assert.IsTrue(grid.IsVisible && grid.IsReadOnly);
                Assert.IsFalse(((TextBox)window.FindName("SearchBox")).IsEnabled);
                Assert.IsFalse(ApplicationCommands.Find.CanExecute(null, window));
                Assert.AreEqual(before, File.ReadAllText(fixture.Path));
                Assert.AreEqual(0, ipc.ReloadCount);
                Assert.AreEqual(vm.ImportPreview!.Changes.Count, grid.Items.Count);
                SavePreview(window, "Transfer", "preview");
                Click("Cancel");
                Assert.IsFalse(grid.IsVisible);
                Assert.AreEqual(before, File.ReadAllText(fixture.Path));
                Click("Import…"); Click("Apply import");
                Assert.AreEqual("Settings imported.", vm.StatusTitle);
                Assert.AreEqual(28, fixture.Storage.Load(fixture.Path).Triggers.WordCount);
                Assert.AreEqual(1, ipc.ReloadCount);
                Assert.IsTrue(ConfigTransferStorageTests.Scalar(fixture, "select count(*) from learned_correction_rules") is 1L);
                Assert.IsFalse(vm.IsImportPreviewVisible);
                Assert.IsTrue(((TextBox)window.FindName("SearchBox")).IsEnabled);
                Assert.AreEqual("import export", vm.SearchText);
            }
            catch (Exception error) { failure = error; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA); thread.Start();
        Assert.IsTrue(thread.Join(TimeSpan.FromSeconds(25)), "WPF import/export flow timed out.");
        if (failure is not null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    private sealed class TransferDialog(string? importPath, string? exportPath) : IConfigFileDialog
    {
        public string? PickImportPath() => importPath;
        public string? PickExportPath() => exportPath;
    }
}
