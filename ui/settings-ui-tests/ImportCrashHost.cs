using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.Settings.Transfer;

namespace AutoFix.SettingsUi.Tests;

/// <summary>Child-process entry point: stops a real import without running catch/finally compensation.</summary>
internal static class ImportCrashHost
{
    public static int Main(string[] args)
    {
        if (args.Length != 2 || !Enum.TryParse<ImportStage>(args[1], out var stopAt)) return 2;
        var config = new ConfigStorage(Path.Combine(args[0], "settings.toml"));
        var transfer = new ConfigTransferStorage(config, new AppRuleStorage(Path.Combine(args[0], "autofix.sqlite")));
        var preview = transfer.Preview(Path.Combine(args[0], "bundle.zip"));
        transfer.Apply(preview, transaction => transaction.Commit(), stage =>
        {
            if (stage == stopAt) Environment.Exit(73);
            if (stage == ImportStage.Committed && stopAt == ImportStage.RecoveryFileReplaced)
                ImportRecovery.Recover(config.ConfigPath, afterFileReplaced: () => Environment.Exit(73));
        });
        return 3;
    }
}
