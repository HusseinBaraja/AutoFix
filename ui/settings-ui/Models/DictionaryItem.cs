namespace AutoFix.SettingsUi.Models;

public sealed class DictionaryItem
{
    public long Id { get; init; }
    public string Word { get; init; } = "";
    public string Language { get; init; } = "";
    public string Source { get; init; } = "";
    public string App { get; init; } = "";
    public string Replacement { get; init; } = "";
    public string RuleLabel => Source == "pair" ? "Block pair" : "Keep original";
    public string AppLabel => string.IsNullOrEmpty(App) ? "All apps" : App;
}
