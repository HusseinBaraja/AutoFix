namespace AutoFix.SettingsUi.Models;

public sealed record MetadataLogItem(string OccurredAt, string App, string Trigger, string Confidence,
    string Engine, string ReplacementMethod, string Result, long LatencyMs);
