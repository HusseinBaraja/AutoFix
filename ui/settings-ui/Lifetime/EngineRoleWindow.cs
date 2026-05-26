using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Lifetime;

internal sealed class EngineRoleWindow : IDisposable
{
    internal const string Title = "Autofix Engine";
    private const string ClassName = "AutoFix.Engine.RoleWindow";
    private readonly ushort classAtom;
    private readonly NativeMethods.WndProc wndProc;
    private IntPtr handle;

    private EngineRoleWindow(ushort classAtom, NativeMethods.WndProc wndProc, IntPtr handle)
    {
        this.classAtom = classAtom;
        this.wndProc = wndProc;
        this.handle = handle;
    }

    public static EngineRoleWindow? Create()
    {
        if (!OperatingSystem.IsWindows())
        {
            return null;
        }

        NativeMethods.WndProc wndProc = NativeMethods.DefWindowProcW;
        var className = ClassName;
        var windowClass = new NativeMethods.WNDCLASSW
        {
            lpfnWndProc = wndProc,
            lpszClassName = className,
        };
        var atom = NativeMethods.RegisterClassW(windowClass);
        if (atom == 0)
        {
            return null;
        }

        var handle = NativeMethods.CreateWindowExW(
            NativeMethods.WS_EX_TOOLWINDOW | NativeMethods.WS_EX_NOACTIVATE,
            className,
            Title,
            NativeMethods.WS_POPUP,
            -32000,
            -32000,
            1,
            1,
            IntPtr.Zero,
            IntPtr.Zero,
            IntPtr.Zero,
            IntPtr.Zero);
        if (handle == IntPtr.Zero)
        {
            NativeMethods.UnregisterClassW(className, IntPtr.Zero);
            return null;
        }

        NativeMethods.ShowWindow(handle, NativeMethods.SW_SHOWNOACTIVATE);
        return new EngineRoleWindow(atom, wndProc, handle);
    }

    public void Dispose()
    {
        if (handle != IntPtr.Zero)
        {
            NativeMethods.DestroyWindow(handle);
            handle = IntPtr.Zero;
        }

        if (classAtom != 0)
        {
            NativeMethods.UnregisterClassW(ClassName, IntPtr.Zero);
        }

        GC.KeepAlive(wndProc);
    }

    private static partial class NativeMethods
    {
        internal const int SW_SHOWNOACTIVATE = 4;
        internal const int WS_POPUP = unchecked((int)0x80000000);
        internal const int WS_EX_TOOLWINDOW = 0x00000080;
        internal const int WS_EX_NOACTIVATE = 0x08000000;

        internal delegate IntPtr WndProc(IntPtr hWnd, uint message, IntPtr wParam, IntPtr lParam);

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        internal sealed class WNDCLASSW
        {
            public uint style;
            public WndProc? lpfnWndProc;
            public int cbClsExtra;
            public int cbWndExtra;
            public IntPtr hInstance;
            public IntPtr hIcon;
            public IntPtr hCursor;
            public IntPtr hbrBackground;
            public string? lpszMenuName;
            public string? lpszClassName;
        }

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        internal static extern ushort RegisterClassW(WNDCLASSW lpWndClass);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool UnregisterClassW(string lpClassName, IntPtr hInstance);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        internal static extern IntPtr CreateWindowExW(
            int dwExStyle,
            string lpClassName,
            string lpWindowName,
            int dwStyle,
            int x,
            int y,
            int nWidth,
            int nHeight,
            IntPtr hWndParent,
            IntPtr hMenu,
            IntPtr hInstance,
            IntPtr lpParam);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool DestroyWindow(IntPtr hWnd);

        [DllImport("user32.dll")]
        internal static extern IntPtr DefWindowProcW(IntPtr hWnd, uint message, IntPtr wParam, IntPtr lParam);
    }
}
