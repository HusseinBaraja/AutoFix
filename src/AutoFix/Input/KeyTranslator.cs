using System.Text;
using AutoFix.Core.Engine;
using AutoFix.Core.Text;
using static AutoFix.Native.Win32;

namespace AutoFix.Input;

/// <summary>A keyboard event as seen by the low-level hook, kept for replay.</summary>
internal sealed record RawKey(ushort Vk, ushort Scan, bool Up, bool Extended);

/// <summary>A mouse button event as seen by the low-level hook, kept for replay.</summary>
internal sealed record RawMouse(int Message, int X, int Y, uint MouseData);

/// <summary>
/// Classifies hook events and converts keys to the characters the focused app receives,
/// using the focused app's keyboard layout. Tracks modifier state itself because
/// the hook thread's key state does not follow other apps.
/// </summary>
internal sealed class KeyTranslator
{
    // ToUnicodeEx flag: do not change the keyboard state (Windows 10 1607+), so dead keys keep working.
    private const uint NoStateChange = 0x4;

    private readonly bool[] _down = new bool[256];
    private readonly byte[] _state = new byte[256];
    private readonly StringBuilder _chars = new(8);
    private bool _capsLock = (GetKeyState(VK_CAPITAL) & 1) != 0;
    private bool _numLock = (GetKeyState(VK_NUMLOCK) & 1) != 0;
    private char? _deadKey;

    public Mods Mods
    {
        get
        {
            var mods = Mods.None;
            if (_down[VK_LSHIFT] || _down[VK_RSHIFT] || _down[VK_SHIFT]) mods |= Mods.Shift;
            if (_down[VK_LCONTROL] || _down[VK_RCONTROL] || _down[VK_CONTROL]) mods |= Mods.Ctrl;
            if (_down[VK_LMENU] || _down[VK_RMENU] || _down[VK_MENU]) mods |= Mods.Alt;
            if (_down[VK_LWIN] || _down[VK_RWIN]) mods |= Mods.Win;
            return mods;
        }
    }

    /// <summary>Records a key event that reached the focused app.</summary>
    public void Update(RawKey key)
    {
        if (key.Vk >= 256) return;
        if (!key.Up && !_down[key.Vk])
        {
            if (key.Vk == VK_CAPITAL) _capsLock = !_capsLock;
            if (key.Vk == VK_NUMLOCK) _numLock = !_numLock;
        }
        _down[key.Vk] = !key.Up;
    }

    private static readonly int[] ModifierKeys =
        [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN];

    /// <summary>
    /// Clears modifiers whose key-up the hook never saw, for example after Ctrl+Alt+Del, Win+L
    /// or a UAC prompt, where keys are released on another desktop.
    /// </summary>
    public void Resync()
    {
        foreach (int vk in ModifierKeys)
        {
            if (_down[vk] && !IsDown(vk)) _down[vk] = false;
        }
    }

    public KeyStroke Translate(RawKey key, bool isReplay, IntPtr layout)
    {
        var mods = Mods;
        KeyStroke Make(KeyKind kind, string? text = null) => new(kind, !key.Up, text, mods, isReplay, key);

        if (key.Up) return Make(KeyKind.Neutral);
        if (mods != Mods.None)
        {
            Resync();
            mods = Mods;
        }
        int vk = key.Vk;
        if (IsModifier(vk)) return Make(KeyKind.Modifier);
        if (vk == VK_PACKET)
        {
            _deadKey = null;
            return Make(KeyKind.Text, ((char)key.Scan).ToString());
        }

        if (mods.HasFlag(Mods.Win)) return Reset();
        if (mods.HasFlag(Mods.Ctrl) || mods.HasFlag(Mods.Alt))
        {
            // Ctrl+Alt is AltGr on many layouts and can type characters such as @ or €.
            if (mods.HasFlag(Mods.Ctrl) && mods.HasFlag(Mods.Alt)
                && ToText(key, layout) is { Count: > 0 } altGr && IsPrintable(altGr.Text))
            {
                return Make(KeyKind.Text, altGr.Text);
            }
            return Reset();
        }

        switch (vk)
        {
            case VK_BACK:
                _deadKey = null;
                return Make(KeyKind.Backspace);
            case VK_RETURN:
                _deadKey = null;
                return Make(KeyKind.Enter);
            case 0x2E: // Delete: changes text after the caret only.
            case 0x13 or 0x2C or 0x91: // Pause, Print Screen, Scroll Lock
            case >= 0xAD and <= 0xB7: // Volume and media keys
                return Make(KeyKind.Neutral);
            case 0x09 or 0x1B: // Tab, Esc
            case >= 0x21 and <= 0x28: // Page Up/Down, End, Home, arrows
            case 0x2D or 0x5D: // Insert, context menu
            case >= 0x70 and <= 0x87: // F1-F24
            case >= 0xA6 and <= 0xAC: // Browser keys
            case 0xE5: // IME processing
                return Reset();
        }

        var result = ToText(key, layout);
        if (result is null) return Make(KeyKind.Neutral);
        var (count, text) = result.Value;
        if (count < 0)
        {
            if (_deadKey is not null) return Reset();
            _deadKey = text[0];
            return Make(KeyKind.Neutral);
        }
        if (!IsPrintable(text)) return Reset();
        if (_deadKey is { } dead)
        {
            _deadKey = null;
            return DeadKeys.Compose(dead, text) is { } composed ? Make(KeyKind.Text, composed) : Reset();
        }
        return Make(KeyKind.Text, text);

        KeyStroke Reset()
        {
            _deadKey = null;
            return Make(KeyKind.Reset);
        }
    }

    private (int Count, string Text)? ToText(RawKey key, IntPtr layout)
    {
        Array.Clear(_state);
        void Set(int vk, bool down)
        {
            if (down) _state[vk] = 0x80;
        }
        Set(VK_LSHIFT, _down[VK_LSHIFT]);
        Set(VK_RSHIFT, _down[VK_RSHIFT]);
        Set(VK_SHIFT, _down[VK_LSHIFT] || _down[VK_RSHIFT] || _down[VK_SHIFT]);
        Set(VK_LCONTROL, _down[VK_LCONTROL]);
        Set(VK_RCONTROL, _down[VK_RCONTROL]);
        Set(VK_CONTROL, _down[VK_LCONTROL] || _down[VK_RCONTROL] || _down[VK_CONTROL]);
        Set(VK_LMENU, _down[VK_LMENU]);
        Set(VK_RMENU, _down[VK_RMENU]);
        Set(VK_MENU, _down[VK_LMENU] || _down[VK_RMENU] || _down[VK_MENU]);
        if (_capsLock) _state[VK_CAPITAL] = 0x01;
        if (_numLock) _state[VK_NUMLOCK] = 0x01;

        _chars.Clear();
        _chars.Capacity = Math.Max(_chars.Capacity, 8);
        int count = ToUnicodeEx(key.Vk, key.Scan, _state, _chars, 8, NoStateChange, layout);
        if (count == 0) return null;
        string text = _chars.ToString();
        if (count > 0 && text.Length > count) text = text[..count];
        if (text.Length == 0) return null;
        return (count, text);
    }

    private static bool IsPrintable(string text)
    {
        foreach (char c in text)
        {
            if (char.IsControl(c)) return false;
        }
        return true;
    }

    private static bool IsModifier(int vk) => vk is VK_SHIFT or VK_CONTROL or VK_MENU or VK_CAPITAL or VK_NUMLOCK
        or VK_LWIN or VK_RWIN or (>= VK_LSHIFT and <= VK_RMENU);
}
