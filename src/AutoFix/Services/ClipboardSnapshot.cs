using System.Runtime.InteropServices;
using static AutoFix.Native.Win32;

namespace AutoFix.Services;

/// <summary>
/// Saves and restores every memory-based clipboard format, so fixing a selection
/// leaves the user's clipboard as it was. Uses the raw Win32 API, which works on any thread.
/// </summary>
internal sealed class ClipboardSnapshot
{
    private const long MaxTotalBytes = 64L * 1024 * 1024;
    private readonly List<(uint Format, byte[] Data)> _formats = [];

    private ClipboardSnapshot()
    {
    }

    /// <returns>The snapshot, or null when the clipboard is busy or too large to copy.</returns>
    public static ClipboardSnapshot? Capture()
    {
        if (!Open()) return null;
        var snapshot = new ClipboardSnapshot();
        long total = 0;
        try
        {
            uint format = 0;
            while ((format = EnumClipboardFormats(format)) != 0)
            {
                // Formats that hold GDI handles instead of memory cannot be copied this way;
                // Windows synthesizes them again from the memory formats.
                if (format is 2 or 3 or 9 or 14 or 0x80 or 0x82 or 0x83 or 0x8E || format is >= 0x200 and <= 0x3FF) continue;
                IntPtr handle = GetClipboardData(format);
                if (handle == IntPtr.Zero) continue;
                long size = (long)GlobalSize(handle);
                if (size <= 0) continue;
                total += size;
                if (total > MaxTotalBytes) return null;
                IntPtr source = GlobalLock(handle);
                if (source == IntPtr.Zero) continue;
                try
                {
                    var data = new byte[size];
                    Marshal.Copy(source, data, 0, data.Length);
                    snapshot._formats.Add((format, data));
                }
                finally
                {
                    GlobalUnlock(handle);
                }
            }
            return snapshot;
        }
        finally
        {
            CloseClipboard();
        }
    }

    public bool Restore()
    {
        if (!Open()) return false;
        try
        {
            EmptyClipboard();
            foreach (var (format, data) in _formats) Put(format, data);
            // The user's content is already in clipboard history; do not add it a second time.
            ExcludeFromHistory();
            return true;
        }
        finally
        {
            CloseClipboard();
        }
    }

    public static string? GetText()
    {
        if (!Open()) return null;
        try
        {
            IntPtr handle = GetClipboardData(CF_UNICODETEXT);
            if (handle == IntPtr.Zero) return null;
            IntPtr source = GlobalLock(handle);
            if (source == IntPtr.Zero) return null;
            try
            {
                return Marshal.PtrToStringUni(source);
            }
            finally
            {
                GlobalUnlock(handle);
            }
        }
        finally
        {
            CloseClipboard();
        }
    }

    /// <summary>Puts text on the clipboard and keeps it out of clipboard history and cloud sync.</summary>
    public static bool SetText(string text)
    {
        if (!Open()) return false;
        try
        {
            EmptyClipboard();
            byte[] chars = System.Text.Encoding.Unicode.GetBytes(text + "\0");
            Put(CF_UNICODETEXT, chars);
            ExcludeFromHistory();
            return true;
        }
        finally
        {
            CloseClipboard();
        }
    }

    private static void ExcludeFromHistory()
    {
        Put(RegisterClipboardFormat("ExcludeClipboardContentFromMonitorProcessing"), [0, 0, 0, 0]);
        Put(RegisterClipboardFormat("CanIncludeInClipboardHistory"), [0, 0, 0, 0]);
        Put(RegisterClipboardFormat("CanUploadToCloudClipboard"), [0, 0, 0, 0]);
    }

    private static void Put(uint format, byte[] data)
    {
        IntPtr memory = GlobalAlloc(GMEM_MOVEABLE, (UIntPtr)Math.Max(1, data.Length));
        if (memory == IntPtr.Zero) return;
        IntPtr target = GlobalLock(memory);
        if (target == IntPtr.Zero)
        {
            GlobalFree(memory);
            return;
        }
        Marshal.Copy(data, 0, target, data.Length);
        GlobalUnlock(memory);
        // On success the clipboard owns the memory.
        if (SetClipboardData(format, memory) == IntPtr.Zero) GlobalFree(memory);
    }

    private static bool Open()
    {
        for (int attempt = 0; attempt < 20; attempt++)
        {
            if (OpenClipboard(IntPtr.Zero)) return true;
            Thread.Sleep(10);
        }
        return false;
    }
}
