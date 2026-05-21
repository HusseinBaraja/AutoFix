namespace AutoFix.SettingsUi.Models;

public sealed class AppRuleItem
{
    public string App { get; init; } = "";
    public string Scope { get; init; } = "";
    public string Mode { get; init; } = "";
    public string Engine { get; init; } = "";
    public string Notes { get; init; } = "";
}
