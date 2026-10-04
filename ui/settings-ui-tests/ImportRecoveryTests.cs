using System.Diagnostics;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.Settings.Transfer;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ImportRecoveryTests
{
    /// <summary>Actual process termination at each durable boundary recovers settings and permissions to one generation.</summary>
    [DataTestMethod]
    [DataRow("Prepared", false, false)]
    [DataRow("SettingsReplaced", false, false)]
    [DataRow("Committed", true, false)]
    [DataRow("RecoveryFileReplaced", true, false)]
    [DataRow("Prepared", false, true)]
    [DataRow("SettingsReplaced", false, true)]
    [DataRow("Committed", true, true)]
    [DataRow("RecoveryFileReplaced", true, true)]
    public void InterruptedImportRecoversTheDatabaseDecision(string stage, bool committed, bool useWal)
    {
        using var fixture = TempConfigFixture.Create();
        var transfer = ConfigTransferStorageTests.CreateTransfer(fixture);
        if (useWal) ConfigTransferStorageTests.Execute(fixture, "pragma journal_mode = WAL");
        var apps = new AppRuleStorage(ConfigTransferStorageTests.Database(fixture));
        apps.Upsert(new() { ProcessName = "notes.exe", ManualShortcutAllowed = true, ApiEngineAllowed = false });
        var config = AppConfig.Default(); config.Triggers.WordCount = 22;
        var bundle = Path.Combine(fixture.Root, "bundle.zip");
        transfer.Export(bundle, config, true);
        apps.Upsert(new() { ProcessName = "notes.exe", ManualShortcutAllowed = true, ApiEngineAllowed = true });
        new DictionaryStorage(ConfigTransferStorageTests.Database(fixture)).Save(new() { Word = "old-word", Language = "und", Source = "dictionary" });
        var original = File.ReadAllBytes(fixture.Path);
        var start = new ProcessStartInfo("dotnet") { UseShellExecute = false, CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden, RedirectStandardOutput = true, RedirectStandardError = true };
        start.ArgumentList.Add(typeof(ImportCrashHost).Assembly.Location);
        start.ArgumentList.Add(fixture.Root); start.ArgumentList.Add(stage);
        using var process = Process.Start(start)!;
        var output = process.StandardOutput.ReadToEndAsync(); var errors = process.StandardError.ReadToEndAsync();
        if (!process.WaitForExit(15000)) { process.Kill(entireProcessTree: true); Assert.Fail("Import child did not terminate."); }
        Assert.AreEqual(73, process.ExitCode, output.Result + errors.Result);
        Assert.AreEqual(committed ? 1L : 0L, ConfigTransferStorageTests.Scalar(fixture, "select committed from settings_import_recovery"));
        // Simulate a settings file not surviving shutdown; the durable decision must repair it.
        File.Delete(fixture.Path);
        var recovered = new ConfigStorage(fixture.Path).LoadOrCreate();
        Assert.AreEqual(committed ? 22 : 10, recovered.Triggers.WordCount);
        Assert.AreEqual(committed ? 0L : 1L, ConfigTransferStorageTests.Scalar(fixture, "select count(*) from custom_dictionary_entries where entry = 'old-word'"));
        Assert.AreEqual(committed ? 0L : 1L, ConfigTransferStorageTests.Scalar(fixture, "select api_engine_allowed from app_rules where process_name = 'notes.exe'"));
        Assert.AreEqual(0L, ConfigTransferStorageTests.Scalar(fixture, "select count(*) from settings_import_recovery"));
        if (!committed) CollectionAssert.AreEqual(original, File.ReadAllBytes(fixture.Path));
        Assert.AreEqual(recovered.Triggers.WordCount, fixture.Storage.LoadOrCreate().Triggers.WordCount);
    }

    /// <summary>Readers and stale writers refuse the in-progress import lock without changing persistent data.</summary>
    [TestMethod]
    public void SettingsReadersAndWritersHonorTheImportLock()
    {
        using var fixture = TempConfigFixture.Create(); fixture.Storage.LoadOrCreate();
        var original = File.ReadAllBytes(fixture.Path);
        using (ConfigFileAccess.Acquire(fixture.Path))
        {
            Assert.ThrowsException<IOException>(() => fixture.Storage.LoadOrCreate());
            Assert.ThrowsException<IOException>(() => fixture.Storage.Save(AppConfig.Default()));
        }
        CollectionAssert.AreEqual(original, File.ReadAllBytes(fixture.Path));
    }

    /// <summary>Recovered imports are not overwritten by a settings form built from an older generation.</summary>
    [TestMethod]
    public void StaleSaveAfterRecoveryRequiresReload()
    {
        using var fixture = TempConfigFixture.Create(); fixture.Storage.LoadOrCreate();
        var imported = AppConfig.Default(); imported.Triggers.WordCount = 22;
        using (var connection = new Microsoft.Data.Sqlite.SqliteConnection($"Data Source={ConfigTransferStorageTests.Database(fixture)}"))
        {
            connection.Open(); ImportRecovery.EnsureTable(connection);
            using var transaction = connection.BeginTransaction();
            ImportRecovery.Prepare(connection, transaction, File.ReadAllBytes(fixture.Path), System.Text.Encoding.UTF8.GetBytes(ConfigStorage.ToToml(imported)));
            ImportRecovery.MarkCommitted(connection, transaction); transaction.Commit();
        }
        Assert.ThrowsException<InvalidDataException>(() => fixture.Storage.Save(AppConfig.Default()));
        Assert.AreEqual(22, fixture.Storage.LoadOrCreate().Triggers.WordCount);
    }
}
