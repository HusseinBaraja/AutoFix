using System.Globalization;
using AutoFix.Core.Engine;

namespace AutoFix.Core.Text;

/// <summary>
/// Turns a correction into the smallest "backspace then type" edit at the caret.
/// Only the text from the first changed character onward is touched.
/// </summary>
public static class EditPlanner
{
    /// <param name="original">Text currently on screen that the fix replaces.</param>
    /// <param name="corrected">Replacement for <paramref name="original"/>.</param>
    /// <param name="suffix">Text typed after <paramref name="original"/>; it must be typed again.</param>
    /// <returns>The plan, or null when the edit cannot be done safely with backspaces.</returns>
    public static EditPlan? Plan(string original, string corrected, string suffix)
    {
        int keep = CommonPrefixLength(original, corrected);
        string deleted = string.Concat(original.AsSpan(keep), suffix);
        string insert = string.Concat(corrected.AsSpan(keep), suffix);
        if (deleted.Length == 0 && insert.Length == 0) return null;
        if (!CanDelete(deleted) || !CanType(insert)) return null;
        return new EditPlan(deleted.Length, insert);
    }

    public static int CommonPrefixLength(string a, string b)
    {
        int max = Math.Min(a.Length, b.Length);
        int i = 0;
        while (i < max && a[i] == b[i]) i++;
        // Never split a surrogate pair.
        if (i > 0 && i < a.Length && char.IsHighSurrogate(a[i - 1])) i--;
        return i;
    }

    /// <summary>
    /// One backspace removes one character only for plain characters. Emoji, combining
    /// marks and joiners can be removed as a group, which would make the count wrong.
    /// </summary>
    private static bool CanDelete(string text)
    {
        foreach (char c in text)
        {
            if (char.IsControl(c) || char.IsSurrogate(c)) return false;
            // Format covers zero-width joiners and direction marks.
            switch (char.GetUnicodeCategory(c))
            {
                case UnicodeCategory.NonSpacingMark:
                case UnicodeCategory.SpacingCombiningMark:
                case UnicodeCategory.EnclosingMark:
                case UnicodeCategory.Format:
                    return false;
            }
        }
        return true;
    }

    private static bool CanType(string text)
    {
        foreach (char c in text)
        {
            if (char.IsControl(c)) return false;
        }
        return true;
    }
}
