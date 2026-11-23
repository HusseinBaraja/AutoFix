using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Text;
using static AutoFix.Native.Win32;

namespace AutoFix.Input;

internal sealed record FocusTarget(IntPtr Window, uint ProcessId, string ProcessName, bool Elevated, bool IsSelf)
{
    public static readonly FocusTarget None = new(IntPtr.Zero, 0, "", false, false);
}

/// <summary>Facts about the focused window: owning app, elevation, password fields, caret position.</summary>
internal static class FocusInspector
{
    private static readonly uint SelfProcessId = (uint)Environment.ProcessId;
    private static readonly bool SelfElevated = IsElevated(SelfProcessId);
    private static readonly ConcurrentDictionary<uint, (string Name, bool Elevated)> Processes = new();

    public static FocusTarget Describe(IntPtr window)
    {
        if (window == IntPtr.Zero) return FocusTarget.None;
        GetWindowThreadProcessId(window, out uint pid);
        if (Processes.Count > 256) Processes.Clear();
        var (name, elevated) = Processes.GetOrAdd(pid, id => (ProcessName(id), IsElevated(id)));
        return new FocusTarget(window, pid, name, elevated && !SelfElevated, pid == SelfProcessId);
    }

    /// <summary>Forget cached names for a process id that Windows may reuse.</summary>
    public static void Forget(uint processId) => Processes.TryRemove(processId, out _);

    private static string ProcessName(uint pid)
    {
        IntPtr process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if (process == IntPtr.Zero) return "";
        try
        {
            var buffer = new StringBuilder(1024);
            uint size = (uint)buffer.Capacity;
            return QueryFullProcessImageName(process, 0, buffer, ref size) ? Path.GetFileName(buffer.ToString()) : "";
        }
        finally
        {
            CloseHandle(process);
        }
    }

    private static bool IsElevated(uint pid)
    {
        IntPtr process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        // Elevated processes still allow limited queries; failure means we cannot touch it.
        if (process == IntPtr.Zero) return true;
        try
        {
            if (!OpenProcessToken(process, TOKEN_QUERY, out IntPtr token)) return true;
            try
            {
                return GetTokenInformation(token, TokenElevation, out int elevated, sizeof(int), out _) && elevated != 0;
            }
            finally
            {
                CloseHandle(token);
            }
        }
        finally
        {
            CloseHandle(process);
        }
    }

    /// <summary>
    /// True when the focused control is a password field. Checks classic Win32 edits, then
    /// UI Automation, which covers browsers and modern apps. Unknown results count as safe
    /// to read, except when UI Automation cannot answer in time.
    /// </summary>
    public static bool IsPasswordField()
    {
        IntPtr foreground = GetForegroundWindow();
        uint thread = GetWindowThreadProcessId(foreground, out _);
        var info = new GUITHREADINFO { cbSize = Marshal.SizeOf<GUITHREADINFO>() };
        if (GetGUIThreadInfo(thread, ref info) && info.hwndFocus != IntPtr.Zero)
        {
            long style = GetWindowLongPtr(info.hwndFocus, GWL_STYLE).ToInt64();
            var className = new StringBuilder(64);
            GetClassName(info.hwndFocus, className, className.Capacity);
            if ((style & ES_PASSWORD) != 0 && className.ToString().Contains("Edit", StringComparison.OrdinalIgnoreCase))
                return true;
        }

        var check = Task.Run(() =>
        {
            try
            {
                return System.Windows.Automation.AutomationElement.FocusedElement?.Current.IsPassword ?? false;
            }
            catch (Exception ex) when (ex is COMException or InvalidOperationException
                or System.Windows.Automation.ElementNotAvailableException)
            {
                return false;
            }
        });
        // If the app does not answer quickly, treat the field as private.
        return !check.Wait(TimeSpan.FromMilliseconds(1000)) || check.Result;
    }

    /// <summary>Caret rectangle in screen pixels, or null when the app does not expose it.</summary>
    public static RECT? CaretRect(IntPtr window)
    {
        uint thread = GetWindowThreadProcessId(window, out _);
        var info = new GUITHREADINFO { cbSize = Marshal.SizeOf<GUITHREADINFO>() };
        if (GetGUIThreadInfo(thread, ref info) && info.hwndCaret != IntPtr.Zero
            && info.rcCaret.Bottom > info.rcCaret.Top)
        {
            var point = new POINT { X = info.rcCaret.Left, Y = info.rcCaret.Top };
            ClientToScreen(info.hwndCaret, ref point);
            return new RECT
            {
                Left = point.X,
                Top = point.Y,
                Right = point.X + (info.rcCaret.Right - info.rcCaret.Left),
                Bottom = point.Y + (info.rcCaret.Bottom - info.rcCaret.Top),
            };
        }

        // Browsers and Electron apps expose the caret through accessibility instead.
        IntPtr focus = info.hwndFocus != IntPtr.Zero ? info.hwndFocus : window;
        try
        {
            var iid = new Guid("618736E0-3C3D-11CF-810C-00AA00389B71");
            if (AccessibleObjectFromWindow(focus, OBJID_CARET, ref iid, out object accessible) == 0
                && accessible is Accessibility.IAccessible caret)
            {
                caret.accLocation(out int left, out int top, out int width, out int height, 0);
                Marshal.ReleaseComObject(caret);
                if (height > 0 && (left != 0 || top != 0))
                    return new RECT { Left = left, Top = top, Right = left + Math.Max(width, 1), Bottom = top + height };
            }
        }
        catch (Exception ex) when (ex is COMException or ArgumentException or InvalidCastException)
        {
            // Fall through: no caret position.
        }
        return null;
    }

    private const uint OBJID_CARET = 0xFFFFFFF8;

    [DllImport("oleacc.dll")]
    private static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint id, ref Guid iid,
        [MarshalAs(UnmanagedType.IUnknown)] out object accessible);
}
