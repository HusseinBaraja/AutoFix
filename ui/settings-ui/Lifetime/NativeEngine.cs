using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Lifetime;

internal static partial class NativeEngine
{
    internal static int RunBackground() => NativeMethods.autofix_run_background();

    private static partial class NativeMethods
    {
        [DllImport("autofix_core", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autofix_run_background();
    }
}
