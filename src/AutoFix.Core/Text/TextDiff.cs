namespace AutoFix.Core.Text;

public readonly record struct WordChange(string From, string To);

/// <summary>Word-level differences, used to show what changed and to learn from undo.</summary>
public static class TextDiff
{
    public static IReadOnlyList<WordChange> ChangedWords(string original, string corrected)
    {
        var a = Tokenize(original);
        var b = Tokenize(corrected);

        // Longest common subsequence over tokens.
        var lcs = new int[a.Count + 1, b.Count + 1];
        for (int i = a.Count - 1; i >= 0; i--)
        {
            for (int j = b.Count - 1; j >= 0; j--)
            {
                lcs[i, j] = a[i] == b[j] ? lcs[i + 1, j + 1] + 1 : Math.Max(lcs[i + 1, j], lcs[i, j + 1]);
            }
        }

        var changes = new List<WordChange>();
        var from = new List<string>();
        var to = new List<string>();
        int x = 0, y = 0;
        while (x < a.Count || y < b.Count)
        {
            if (x < a.Count && y < b.Count && a[x] == b[y])
            {
                Flush();
                x++;
                y++;
            }
            else if (y < b.Count && (x == a.Count || lcs[x, y + 1] >= lcs[x + 1, y]))
            {
                to.Add(b[y++]);
            }
            else
            {
                from.Add(a[x++]);
            }
        }
        Flush();
        return changes;

        void Flush()
        {
            if (from.Count == 0 && to.Count == 0) return;
            changes.Add(new WordChange(Join(from), Join(to)));
            from.Clear();
            to.Clear();
        }
    }

    /// <summary>Original words that a fix changed. Punctuation-only tokens are skipped.</summary>
    public static IReadOnlyList<string> ChangedOriginalWords(string original, string corrected)
    {
        var words = new List<string>();
        foreach (var change in ChangedWords(original, corrected))
        {
            foreach (var token in Tokenize(change.From))
            {
                if (token.Any(char.IsLetter) && !words.Contains(token, StringComparer.OrdinalIgnoreCase))
                    words.Add(token);
            }
        }
        return words;
    }

    /// <summary>Words (letters, digits, apostrophes) and single punctuation marks.</summary>
    internal static List<string> Tokenize(string text)
    {
        var tokens = new List<string>();
        int i = 0;
        while (i < text.Length)
        {
            char c = text[i];
            if (char.IsWhiteSpace(c))
            {
                i++;
                continue;
            }
            if (IsWordChar(c))
            {
                int start = i;
                while (i < text.Length && (IsWordChar(text[i]) || IsInnerApostrophe(text, i))) i++;
                tokens.Add(text[start..i]);
                continue;
            }
            int len = char.IsHighSurrogate(c) && i + 1 < text.Length ? 2 : 1;
            tokens.Add(text.Substring(i, len));
            i += len;
        }
        return tokens;
    }

    private static bool IsWordChar(char c) => char.IsLetterOrDigit(c) || char.GetUnicodeCategory(c) is
        System.Globalization.UnicodeCategory.NonSpacingMark or System.Globalization.UnicodeCategory.SpacingCombiningMark;

    private static bool IsInnerApostrophe(string text, int i) =>
        text[i] is '\'' or '’' && i > 0 && i + 1 < text.Length && IsWordChar(text[i - 1]) && IsWordChar(text[i + 1]);

    private static string Join(List<string> tokens)
    {
        var builder = new System.Text.StringBuilder();
        foreach (var token in tokens)
        {
            bool attach = builder.Length > 0 && token.Length == 1 && char.IsPunctuation(token[0]);
            if (builder.Length > 0 && !attach) builder.Append(' ');
            builder.Append(token);
        }
        return builder.ToString();
    }
}
