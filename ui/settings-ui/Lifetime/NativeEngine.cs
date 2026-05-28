using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Lifetime;

internal static partial class NativeEngine
{
    internal const int NativeStartFailedExitCode = 70;

    internal static int RunBackground()
    {
        try
        {
            return NativeMethods.autofix_run_background();
        }
        catch (DllNotFoundException error)
        {
            System.Diagnostics.Debug.WriteLine(error);
            return NativeStartFailedExitCode;
        }
        catch (EntryPointNotFoundException error)
        {
            System.Diagnostics.Debug.WriteLine(error);
            return NativeStartFailedExitCode;
        }
    }

    private static partial class NativeMethods
    {
        [DllImport("autofix_core", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autofix_run_background();
    }
}
