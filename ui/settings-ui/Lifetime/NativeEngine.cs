using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Lifetime;

internal static partial class NativeEngine
{
    internal static int RunBackground() => NativeMethods.autofix_run_background();

    internal static int ShutdownAll() => NativeMethods.autofix_shutdown_all();

    private static partial class NativeMethods
    {
        [DllImport("autofix_core", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autofix_run_background();

        [DllImport("autofix_core", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autofix_shutdown_all();
    }
}
