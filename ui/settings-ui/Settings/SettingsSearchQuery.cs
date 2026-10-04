using System.Text.RegularExpressions;

namespace AutoFix.SettingsUi.Settings;

/// <summary>Matches public setting metadata only, with every query term required.</summary>
internal sealed class SettingsSearchQuery
{
    public SettingsSearchQuery(string query)
    {
        IsEmpty = string.IsNullOrWhiteSpace(query);
        Terms = Regex.Matches(query, @"[\p{L}\p{N}]+")
            .Select(match => match.Value).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
    }

    public bool IsEmpty { get; }
    public IReadOnlyList<string> Terms { get; }

    public bool Matches(params string[] fields) => IsEmpty || Terms.Count > 0
        && Terms.All(term => fields.Any(field => field.Contains(term, StringComparison.OrdinalIgnoreCase)));

    public int Score(string title, string description, string path, string sectionName)
    {
        return Terms.Sum(term => Weight(title, term) * 50 + Weight(description, term) * 10
            + Weight(path, term) * 5 + Weight(sectionName, term) * 20);
    }

    private static int Weight(string text, string term) => text.Equals(term, StringComparison.OrdinalIgnoreCase) ? 4
        : text.StartsWith(term, StringComparison.OrdinalIgnoreCase) ? 3
        : text.Contains(term, StringComparison.OrdinalIgnoreCase) ? 1 : 0;
}
