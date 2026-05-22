using System.Windows.Input;

namespace AutoFix.SettingsUi.Settings;

public static class HotkeyFormatter
{
    public static string Format(Key key, ModifierKeys modifiers)
    {
        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt
            or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin
            or Key.System or Key.None)
        {
            return "";
        }

        var parts = new List<string>();
        if (modifiers.HasFlag(ModifierKeys.Control))
        {
            parts.Add("Ctrl");
        }
        if (modifiers.HasFlag(ModifierKeys.Alt))
        {
            parts.Add("Alt");
        }
        if (modifiers.HasFlag(ModifierKeys.Shift))
        {
            parts.Add("Shift");
        }
        if (modifiers.HasFlag(ModifierKeys.Windows))
        {
            parts.Add("Win");
        }
        if (parts.Count == 0)
        {
            return "";
        }

        parts.Add(KeyName(key));
        return string.Join("+", parts);
    }

    public static bool IsValid(string hotkey) => TryParse(hotkey, out _);

    public static bool Conflicts(string left, string right) =>
        TryParse(left, out var normalizedLeft)
        && TryParse(right, out var normalizedRight)
        && normalizedLeft == normalizedRight;

    private static bool TryParse(string hotkey, out string normalized)
    {
        normalized = "";
        var parts = hotkey
            .Split('+', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 2)
        {
            return false;
        }

        var modifiers = ModifierKeys.None;
        var seen = ModifierKeys.None;
        string? key = null;
        foreach (var part in parts)
        {
            switch (part.ToLowerInvariant())
            {
                case "ctrl":
                case "control":
                    if ((seen & ModifierKeys.Control) != 0)
                    {
                        return false;
                    }
                    seen |= ModifierKeys.Control;
                    modifiers |= ModifierKeys.Control;
                    break;
                case "alt":
                    if ((seen & ModifierKeys.Alt) != 0)
                    {
                        return false;
                    }
                    seen |= ModifierKeys.Alt;
                    modifiers |= ModifierKeys.Alt;
                    break;
                case "shift":
                    if ((seen & ModifierKeys.Shift) != 0)
                    {
                        return false;
                    }
                    seen |= ModifierKeys.Shift;
                    modifiers |= ModifierKeys.Shift;
                    break;
                case "win":
                case "windows":
                case "meta":
                    if ((seen & ModifierKeys.Windows) != 0)
                    {
                        return false;
                    }
                    seen |= ModifierKeys.Windows;
                    modifiers |= ModifierKeys.Windows;
                    break;
                default:
                    if (key is not null || !IsSupportedKey(part))
                    {
                        return false;
                    }
                    key = CanonicalKey(part);
                    break;
            }
        }

        if (modifiers == ModifierKeys.None || key is null)
        {
            return false;
        }

        normalized = string.Join("+", ModifierNames(modifiers).Append(key));
        return true;
    }

    private static IEnumerable<string> ModifierNames(ModifierKeys modifiers)
    {
        if (modifiers.HasFlag(ModifierKeys.Control))
        {
            yield return "Ctrl";
        }
        if (modifiers.HasFlag(ModifierKeys.Alt))
        {
            yield return "Alt";
        }
        if (modifiers.HasFlag(ModifierKeys.Shift))
        {
            yield return "Shift";
        }
        if (modifiers.HasFlag(ModifierKeys.Windows))
        {
            yield return "Win";
        }
    }

    private static string KeyName(Key key) => key switch
    {
        Key.Space => "Space",
        >= Key.A and <= Key.Z => key.ToString(),
        >= Key.D0 and <= Key.D9 => key.ToString()[1..],
        >= Key.F1 and <= Key.F24 => key.ToString(),
        _ => "",
    };

    private static bool IsSupportedKey(string value)
    {
        var upper = value.ToUpperInvariant();
        return upper == "SPACE"
            || (upper.Length == 1 && char.IsAsciiLetterOrDigit(upper[0]))
            || (upper.StartsWith('F') && int.TryParse(upper[1..], out var number) && number is >= 1 and <= 24);
    }

    private static string CanonicalKey(string value)
    {
        var upper = value.ToUpperInvariant();
        return upper == "SPACE" ? "Space" : upper;
    }
}
