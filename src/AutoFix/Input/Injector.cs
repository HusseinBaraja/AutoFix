using AutoFix.Core.Engine;
using static AutoFix.Native.Win32;

namespace AutoFix.Input;

/// <summary>Builds <c>SendInput</c> batches. One batch is never interleaved with real user input.</summary>
internal static class Injector
{
    /// <summary>Marks AutoFix's own edits so the hook can count them.</summary>
    public static readonly UIntPtr EditMarker = 0x41464544; // "AFED"
    /// <summary>Marks user input that AutoFix held back and sends again.</summary>
    public static readonly UIntPtr ReplayMarker = 0x41465250; // "AFRP"
    /// <summary>Marks Ctrl+C / Ctrl+V sent for selection fixes; the hook ignores them.</summary>
    public static readonly UIntPtr CommandMarker = 0x4146434D; // "AFCM"

    public static INPUT[] BuildEdit(EditPlan plan)
    {
        var inputs = new List<INPUT>(plan.Backspaces * 2 + plan.Insert.Length * 2);
        ushort backScan = (ushort)MapVirtualKey(VK_BACK, 0);
        for (int i = 0; i < plan.Backspaces; i++)
        {
            inputs.Add(Key(VK_BACK, backScan, 0, EditMarker));
            inputs.Add(Key(VK_BACK, backScan, KEYEVENTF_KEYUP, EditMarker));
        }
        foreach (char c in plan.Insert)
        {
            inputs.Add(Key(0, c, KEYEVENTF_UNICODE, EditMarker));
            inputs.Add(Key(0, c, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, EditMarker));
        }
        return [.. inputs];
    }

    public static INPUT[] BuildReplay(IEnumerable<object> events)
    {
        var inputs = new List<INPUT>();
        foreach (var item in events)
        {
            switch (item)
            {
                case RawKey { Vk: VK_PACKET } key:
                    inputs.Add(Key(0, key.Scan, KEYEVENTF_UNICODE | (key.Up ? KEYEVENTF_KEYUP : 0), ReplayMarker));
                    break;
                case RawKey key:
                    uint flags = (key.Up ? KEYEVENTF_KEYUP : 0) | (key.Extended ? KEYEVENTF_EXTENDEDKEY : 0);
                    inputs.Add(Key(key.Vk, key.Scan, flags, ReplayMarker));
                    break;
                case RawMouse mouse:
                    inputs.Add(Mouse(mouse));
                    break;
            }
        }
        return [.. inputs];
    }

    /// <summary>Ctrl + key, for copy and paste.</summary>
    public static INPUT[] BuildCtrlChord(ushort vk) =>
    [
        Key(VK_CONTROL, 0, 0, CommandMarker),
        Key(vk, 0, 0, CommandMarker),
        Key(vk, 0, KEYEVENTF_KEYUP, CommandMarker),
        Key(VK_CONTROL, 0, KEYEVENTF_KEYUP, CommandMarker),
    ];

    public static bool Send(INPUT[] inputs) =>
        inputs.Length == 0 || SendInput((uint)inputs.Length, inputs, System.Runtime.InteropServices.Marshal.SizeOf<INPUT>()) == inputs.Length;

    private static INPUT Key(ushort vk, ushort scan, uint flags, UIntPtr marker) => new()
    {
        type = INPUT_KEYBOARD,
        U = new InputUnion { ki = new KEYBDINPUT { wVk = vk, wScan = scan, dwFlags = flags, dwExtraInfo = marker } },
    };

    private static INPUT Mouse(RawMouse mouse)
    {
        int left = GetSystemMetrics(76), top = GetSystemMetrics(77);
        int width = Math.Max(2, GetSystemMetrics(78)), height = Math.Max(2, GetSystemMetrics(79));
        uint button = mouse.Message switch
        {
            WM_LBUTTONDOWN => MOUSEEVENTF_LEFTDOWN,
            WM_LBUTTONUP => MOUSEEVENTF_LEFTUP,
            WM_RBUTTONDOWN => MOUSEEVENTF_RIGHTDOWN,
            WM_RBUTTONUP => MOUSEEVENTF_RIGHTUP,
            WM_MBUTTONDOWN => MOUSEEVENTF_MIDDLEDOWN,
            WM_MBUTTONUP => MOUSEEVENTF_MIDDLEUP,
            WM_XBUTTONDOWN => MOUSEEVENTF_XDOWN,
            WM_XBUTTONUP => MOUSEEVENTF_XUP,
            _ => 0,
        };
        return new INPUT
        {
            type = INPUT_MOUSE,
            U = new InputUnion
            {
                mi = new MOUSEINPUT
                {
                    dx = (int)((mouse.X - left) * 65535L / (width - 1)),
                    dy = (int)((mouse.Y - top) * 65535L / (height - 1)),
                    mouseData = mouse.Message is WM_XBUTTONDOWN or WM_XBUTTONUP ? mouse.MouseData >> 16 : 0,
                    dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | button,
                    dwExtraInfo = ReplayMarker,
                },
            },
        };
    }
}
