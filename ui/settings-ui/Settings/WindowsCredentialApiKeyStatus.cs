using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Settings;

public sealed class WindowsCredentialApiKeyStatus : IApiKeyStatus
{
    private const int GenericCredential = 1;

    public bool HasConfiguredApiKey(AppConfig config)
    {
        var target = $"AutoFix/provider-profile/{config.Api.ProviderPreset}";
        if (!CredRead(target, GenericCredential, 0, out var credential))
        {
            return false;
        }

        try
        {
            return Marshal.ReadInt32(credential, CredentialBlobSizeOffset) > 0;
        }
        finally
        {
            CredFree(credential);
        }
    }

    // CREDENTIALW fields before CredentialBlobSize: Flags, Type, TargetName,
    // Comment, FILETIME. Pointer alignment differs between x86 and x64.
    private static readonly int CredentialBlobSizeOffset = Marshal.OffsetOf<Credential>(nameof(Credential.CredentialBlobSize)).ToInt32();

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct Credential
    {
        public int Flags;
        public int Type;
        public nint TargetName;
        public nint Comment;
        public long LastWritten;
        public int CredentialBlobSize;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CredRead(string target, int type, int reserved, out nint credential);

    [DllImport("advapi32.dll")]
    private static extern void CredFree(nint credential);
}
