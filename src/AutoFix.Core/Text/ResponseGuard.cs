using System.Text.RegularExpressions;

namespace AutoFix.Core.Text;

/// <summary>
/// Cleans model output and rejects answers that are not a small correction
/// (for example, the model answered a question in the text instead of fixing it).
/// </summary>
public static partial class ResponseGuard
{
    /// <returns>The corrected text, or null when the response must not be applied.</returns>
    public static string? Clean(string original, string response, bool grammar)
    {
        string text = ThinkBlock().Replace(response, "");
        text = text.Trim();
        text = Unwrap(text, original);
        if (text.Length == 0) return null;
        if (text == original) return original;

        if (Count(text, '\n') != Count(original, '\n')) return null;
        if (text.Length > original.Length * 1.6 + 12) return null;

        if (!grammar) text = KeepStyle(original, text);

        int allowed = Math.Max(2, (int)Math.Ceiling(original.Length * (grammar ? 0.5 : 0.34)));
        if (EditDistance(original, text, allowed) > allowed) return null;
        return text;
    }

    /// <summary>
    /// Undoes a change that only extends the last word ("goin" to "going"): after a pause, that word
    /// may be unfinished, and completing it would clash with the letters the user types next.
    /// </summary>
    public static string KeepUnfinishedLastWord(string original, string corrected)
    {
        int originalStart = LastWordStart(original);
        int correctedStart = LastWordStart(corrected);
        string originalWord = original[originalStart..];
        string correctedWord = corrected[correctedStart..];
        bool extendsOnly = originalWord.Length > 0 && correctedWord.Length > originalWord.Length
            && correctedWord.StartsWith(originalWord, StringComparison.OrdinalIgnoreCase);
        return extendsOnly ? corrected[..correctedStart] + originalWord : corrected;
    }

    private static int LastWordStart(string text)
    {
        int i = text.Length;
        while (i > 0 && char.IsLetterOrDigit(text[i - 1])) i--;
        return i;
    }

    private static string Unwrap(string text, string original)
    {
        var fence = CodeFence().Match(text);
        if (fence.Success && !original.Contains("```")) text = fence.Groups[1].Value.Trim();

        var tagged = TextTag().Match(text);
        if (tagged.Success) text = tagged.Groups[1].Value.Trim();

        foreach (var prefix in new[] { "Corrected text:", "Corrected:", "Output:", "Text:" })
        {
            if (text.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)
                && !original.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
            {
                text = text[prefix.Length..].TrimStart();
            }
        }

        foreach (var (open, close) in new[] { ('"', '"'), ('\'', '\''), ('“', '”'), ('«', '»'), ('`', '`') })
        {
            bool wrapped = text.Length >= 2 && text[0] == open && text[^1] == close;
            bool originalWrapped = original.Length >= 2 && original[0] == open && original[^1] == close;
            if (wrapped && !originalWrapped) text = text[1..^1];
        }
        return text;
    }

    /// <summary>In spelling mode, keep the writer's sentence capitalization and final punctuation.</summary>
    private static string KeepStyle(string original, string text)
    {
        if (original.Length > 0 && text.Length > 0
            && char.IsLower(original[0]) && text[0] == char.ToUpperInvariant(original[0])
            && !IsStandaloneI(original))
        {
            text = char.ToLowerInvariant(text[0]) + text[1..];
        }

        char last = original.Length > 0 ? original[^1] : '\0';
        if (text.Length > 1 && text[^1] is '.' or '!' && !".!?…".Contains(last))
        {
            text = text[..^1];
        }
        return text;
    }

    private static bool IsStandaloneI(string text) =>
        text[0] == 'i' && (text.Length == 1 || text[1] is ' ' or '\'' or '’');

    private static int Count(string text, char c)
    {
        int n = 0;
        foreach (char x in text) if (x == c) n++;
        return n;
    }

    /// <summary>Optimal string alignment distance (adjacent swaps cost 1). Stops early above <paramref name="limit"/>.</summary>
    public static int EditDistance(string a, string b, int limit = int.MaxValue)
    {
        if (Math.Abs(a.Length - b.Length) > limit) return limit + 1;
        var prev2 = new int[b.Length + 1];
        var prev = new int[b.Length + 1];
        var cur = new int[b.Length + 1];
        for (int j = 0; j <= b.Length; j++) prev[j] = j;
        for (int i = 1; i <= a.Length; i++)
        {
            cur[0] = i;
            int rowMin = cur[0];
            for (int j = 1; j <= b.Length; j++)
            {
                int cost = a[i - 1] == b[j - 1] ? 0 : 1;
                int value = Math.Min(Math.Min(prev[j] + 1, cur[j - 1] + 1), prev[j - 1] + cost);
                if (i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1])
                    value = Math.Min(value, prev2[j - 2] + 1);
                cur[j] = value;
                rowMin = Math.Min(rowMin, value);
            }
            if (rowMin > limit) return limit + 1;
            (prev2, prev, cur) = (prev, cur, prev2);
        }
        return prev[b.Length];
    }

    [GeneratedRegex(@"<think>.*?</think>", RegexOptions.Singleline)]
    private static partial Regex ThinkBlock();

    [GeneratedRegex(@"^```[a-zA-Z]*\s*\n?(.*?)\n?```$", RegexOptions.Singleline)]
    private static partial Regex CodeFence();

    [GeneratedRegex(@"^<text>\s*(.*?)\s*</text>$", RegexOptions.Singleline)]
    private static partial Regex TextTag();
}
