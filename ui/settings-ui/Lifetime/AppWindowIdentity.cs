using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace AutoFix.SettingsUi.Lifetime;

internal static class AppWindowIdentity
{
    internal const string AppDisplayName = "Autofix";

    internal static void Apply(Window window)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        window.SourceInitialized += (_, _) => ApplyToHandle(new WindowInteropHelper(window).Handle);
    }

    internal static void ApplyToHandle(IntPtr windowHandle)
    {
        if (windowHandle == IntPtr.Zero)
        {
            return;
        }

        var result = NativeMethods.SHGetPropertyStoreForWindow(
            windowHandle,
            NativeMethods.IID_IPropertyStore,
            out var propertyStore);
        if (result < 0)
        {
            return;
        }

        using var appId = PropVariant.FromString(AppIdentity.AppUserModelId);
        using var relaunchName = PropVariant.FromString(AppDisplayName);
        using var relaunchCommand = PropVariant.FromString(Quote(
            Environment.ProcessPath ?? Path.Combine(AppContext.BaseDirectory, "Autofix.exe")));

        propertyStore.SetValue(NativeMethods.PKEY_AppUserModel_ID, appId);
        propertyStore.SetValue(NativeMethods.PKEY_AppUserModel_RelaunchDisplayNameResource, relaunchName);
        propertyStore.SetValue(NativeMethods.PKEY_AppUserModel_RelaunchCommand, relaunchCommand);
        propertyStore.Commit();
    }

    internal static string Quote(string value) => $"\"{value.Replace("\"", "\"\"")}\"";

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    internal readonly struct PropertyKey
    {
        public PropertyKey(Guid formatId, uint propertyId)
        {
            FormatId = formatId;
            PropertyId = propertyId;
        }

        public Guid FormatId { get; }
        public uint PropertyId { get; }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal sealed class PropVariant : IDisposable
    {
        private const ushort VtLpWStr = 31;
        private ushort valueType;
        private ushort reserved1;
        private ushort reserved2;
        private ushort reserved3;
        private IntPtr pointerValue;

        private PropVariant(string value)
        {
            valueType = VtLpWStr;
            pointerValue = Marshal.StringToCoTaskMemUni(value);
        }

        public static PropVariant FromString(string value) => new(value);

        public void Dispose()
        {
            if (pointerValue != IntPtr.Zero)
            {
                Marshal.FreeCoTaskMem(pointerValue);
                pointerValue = IntPtr.Zero;
            }
        }
    }

    [ComImport]
    [Guid("00000138-0000-0000-C000-000000000046")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IPropertyStore
    {
        uint GetCount();
        void GetAt(uint propertyIndex, out PropertyKey key);
        void GetValue(ref PropertyKey key, PropVariant value);
        void SetValue(PropertyKey key, PropVariant value);
        void Commit();
    }

    private static partial class NativeMethods
    {
        internal static readonly Guid IID_IPropertyStore = new("00000138-0000-0000-C000-000000000046");
        internal static readonly PropertyKey PKEY_AppUserModel_ID =
            new(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), 5);
        internal static readonly PropertyKey PKEY_AppUserModel_RelaunchDisplayNameResource =
            new(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), 4);
        internal static readonly PropertyKey PKEY_AppUserModel_RelaunchCommand =
            new(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), 2);

        [DllImport("shell32.dll")]
        internal static extern int SHGetPropertyStoreForWindow(
            IntPtr hwnd,
            [MarshalAs(UnmanagedType.LPStruct)] Guid riid,
            out IPropertyStore propertyStore);
    }
}
