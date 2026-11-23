using AutoFix.Core.Engine;

namespace AutoFix.Core.Settings;

/// <summary>A global shortcut such as <c>Ctrl+Alt+Space</c>: modifiers plus one virtual key.</summary>
public sealed record KeyChord(Mods Mods, int VirtualKey)
{
    private static readonly Dictionary<string, int> Names = BuildNames();
    private static readonly Dictionary<int, string> Labels = Names
        .GroupBy(pair => pair.Value)
        .ToDictionary(group => group.Key, group => group.First().Key);

    public static KeyChord? TryParse(string? text)
    {
        if (string.IsNullOrWhiteSpace(text)) return null;
        var mods = Mods.None;
        int? key = null;
        foreach (var raw in text.Split('+', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries))
        {
            switch (raw.ToLowerInvariant())
            {
                case "ctrl" or "control": mods |= Mods.Ctrl; break;
                case "alt": mods |= Mods.Alt; break;
                case "shift": mods |= Mods.Shift; break;
                case "win" or "windows": mods |= Mods.Win; break;
                default:
                    if (key is not null) return null;
                    var match = Names.FirstOrDefault(pair => string.Equals(pair.Key, raw, StringComparison.OrdinalIgnoreCase));
                    if (match.Key is null) return null;
                    key = match.Value;
                    break;
            }
        }
        if (key is null) return null;
        return Create(mods, key.Value);
    }

    /// <summary>Returns null for chords that would break normal typing, such as Shift+A.</summary>
    public static KeyChord? Create(Mods mods, int virtualKey)
    {
        if (!Labels.ContainsKey(virtualKey)) return null;
        bool functionKey = virtualKey is >= 0x70 and <= 0x87;
        if (!functionKey && (mods & (Mods.Ctrl | Mods.Alt | Mods.Win)) == 0) return null;
        return new KeyChord(mods, virtualKey);
    }

    public static string? Label(int virtualKey) => Labels.GetValueOrDefault(virtualKey);

    public override string ToString()
    {
        var parts = new List<string>();
        if (Mods.HasFlag(Mods.Ctrl)) parts.Add("Ctrl");
        if (Mods.HasFlag(Mods.Alt)) parts.Add("Alt");
        if (Mods.HasFlag(Mods.Shift)) parts.Add("Shift");
        if (Mods.HasFlag(Mods.Win)) parts.Add("Win");
        parts.Add(Labels[VirtualKey]);
        return string.Join("+", parts);
    }

    private static Dictionary<string, int> BuildNames()
    {
        var names = new Dictionary<string, int>(StringComparer.Ordinal);
        for (char c = 'A'; c <= 'Z'; c++) names[c.ToString()] = c;
        for (char c = '0'; c <= '9'; c++) names[c.ToString()] = c;
        for (int i = 1; i <= 24; i++) names["F" + i] = 0x6F + i;
        names["Space"] = 0x20;
        names["Enter"] = 0x0D;
        names["Tab"] = 0x09;
        names["Insert"] = 0x2D;
        names["Delete"] = 0x2E;
        names["Home"] = 0x24;
        names["End"] = 0x23;
        names["PageUp"] = 0x21;
        names["PageDown"] = 0x22;
        names["Left"] = 0x25;
        names["Up"] = 0x26;
        names["Right"] = 0x27;
        names["Down"] = 0x28;
        names["`"] = 0xC0;
        names["-"] = 0xBD;
        names["="] = 0xBB;
        names["["] = 0xDB;
        names["]"] = 0xDD;
        names["\\"] = 0xDC;
        names[";"] = 0xBA;
        names["'"] = 0xDE;
        names[","] = 0xBC;
        names["."] = 0xBE;
        names["/"] = 0xBF;
        return names;
    }
}
