namespace AutoFix.SettingsUi.Models;

public sealed record OptionItem(string Label, string Value)
{
    public override string ToString() => Label;
}
