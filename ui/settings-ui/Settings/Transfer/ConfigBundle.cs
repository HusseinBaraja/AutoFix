using AutoFix.SettingsUi.Ipc;

namespace AutoFix.SettingsUi.Settings.Transfer;

internal sealed record BundleManifest(int FormatVersion, bool LearnedRulesIncluded);
internal sealed record DictionaryEntry(string Word, string Language, string? App);
internal sealed record LearnedRule(bool Enabled, string Original, string? Replacement, string RuleType, string? Language, string? App);
internal sealed record LanguageOverride(string Process, string Language);
internal sealed record TransferData(IReadOnlyList<AppRuleDto> AppRules, IReadOnlyList<DictionaryEntry> Dictionary,
    IReadOnlyList<LearnedRule>? LearnedRules, IReadOnlyList<LanguageOverride> LanguageOverrides);
internal sealed record ConfigBundle(AppConfig Settings, TransferData? Data);

public sealed record ImportChange(string Area, string Name, string Before, string After);

public sealed class ConfigImportPreview
{
    internal ConfigImportPreview(string sourceName, ConfigBundle bundle, IReadOnlyList<ImportChange> changes,
        string fingerprint)
    {
        SourceName = sourceName; Bundle = bundle; Changes = changes; Fingerprint = fingerprint;
    }

    public string SourceName { get; }
    public IReadOnlyList<ImportChange> Changes { get; }
    public string Summary => Bundle.Data is null
        ? "Settings-only TOML import. App rules, custom dictionary and learned rules will stay unchanged."
        : "Import replaces settings, app rules, custom dictionary and language overrides. "
            + (Bundle.Data.LearnedRules is null ? "Learned rules were excluded and will stay unchanged." : "Included learned rules replace your current learned rules.");
    public bool EnablesFullTextDebug => Bundle.Settings.Logging.FullTextDebugModeEnabled;
    public bool HasChanges => Changes.Count > 0;
    public string ChangeSummary => Changes.Count switch
    {
        0 => "No changes. The import matches your saved configuration.",
        1 => "1 change to review",
        _ => $"{Changes.Count} changes to review",
    };
    internal ConfigBundle Bundle { get; }
    internal string Fingerprint { get; }
}
