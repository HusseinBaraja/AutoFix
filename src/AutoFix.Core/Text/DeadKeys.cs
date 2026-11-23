using System.Text;

namespace AutoFix.Core.Text;

/// <summary>
/// Reproduces dead-key composition (for example <c>´</c> then <c>e</c> gives <c>é</c>) so the
/// typed-text buffer matches what the application received.
/// </summary>
public static class DeadKeys
{
    /// <summary>Spacing accent produced by a dead key, mapped to its combining mark.</summary>
    private static readonly Dictionary<char, char> CombiningMarks = new()
    {
        ['`'] = '\u0300',
        ['\u00B4'] = '\u0301', // acute
        ['\''] = '\u0301',
        ['^'] = '\u0302',
        ['\u02C6'] = '\u0302',
        ['~'] = '\u0303',
        ['\u02DC'] = '\u0303',
        ['\u00AF'] = '\u0304', // macron
        ['\u02D8'] = '\u0306', // breve
        ['\u02D9'] = '\u0307', // dot above
        ['\u00A8'] = '\u0308', // diaeresis
        ['"'] = '\u0308',
        ['\u02DA'] = '\u030A', // ring
        ['\u00B0'] = '\u030A',
        ['\u02DD'] = '\u030B', // double acute
        ['\u02C7'] = '\u030C', // caron
        ['\u00B8'] = '\u0327', // cedilla
        ['\u02DB'] = '\u0328', // ogonek
    };

    /// <returns>The text Windows produces, or null when the result is not predictable.</returns>
    public static string? Compose(char deadKey, string next)
    {
        if (!CombiningMarks.TryGetValue(deadKey, out char mark)) return null;
        if (next == " ") return deadKey.ToString();
        if (next.Length != 1 || !char.IsLetter(next[0])) return deadKey + next;

        string composed = (next + mark).Normalize(NormalizationForm.FormC);
        return composed.Length == 1 ? composed : deadKey + next;
    }
}
