using System.Runtime.InteropServices;
using System.ComponentModel;
using System.Security;
using System.Security.Cryptography;
using System.Text;

namespace AutoFix.SettingsUi.Settings;

public sealed class WindowsCredentialApiKeyStatus : IApiKeyStore
{
    private const int GenericCredential = 1;

    /// <summary>Checks whether the selected profile has a non-empty Windows credential.</summary>
    public bool HasConfiguredApiKey(AppConfig config)
    {
        var target = Target(config.Api.ProviderPreset);
        if (!CredRead(target, GenericCredential, 0, out var credential))
        {
            var error = Marshal.GetLastWin32Error();
            if (error == 1168) return false;
            throw new Win32Exception(error, "Unable to check the saved API key.");
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

    /// <summary>Writes UTF-8 bytes compatible with the engine; clears temporary secret buffers.</summary>
    public void Save(string providerProfile, SecureString key)
    {
        var target = Target(providerProfile);
        if (key.Length == 0) throw new ArgumentException("Enter an API key before saving.");
        var pointer = Marshal.SecureStringToGlobalAllocUnicode(key);
        var characters = new char[key.Length];
        byte[]? bytes = null;
        GCHandle pinned = default;
        try
        {
            Marshal.Copy(pointer, characters, 0, characters.Length);
            bytes = Encoding.UTF8.GetBytes(characters);
            if (bytes.Length > 2560) throw new ArgumentException("API key exceeds the secure storage limit.");
            pinned = GCHandle.Alloc(bytes, GCHandleType.Pinned);
            var credential = new Credential { Type = GenericCredential, TargetName = Marshal.StringToHGlobalUni(target),
                UserName = Marshal.StringToHGlobalUni("AutoFix"), CredentialBlobSize = bytes.Length,
                CredentialBlob = pinned.AddrOfPinnedObject(), Persist = 2 };
            try
            {
                if (!CredWrite(ref credential, 0))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Unable to save the API key securely.");
            }
            finally
            {
                Marshal.FreeHGlobal(credential.TargetName);
                Marshal.FreeHGlobal(credential.UserName);
            }
        }
        finally
        {
            if (bytes is not null) CryptographicOperations.ZeroMemory(bytes);
            Array.Clear(characters);
            if (pinned.IsAllocated) pinned.Free();
            Marshal.ZeroFreeGlobalAllocUnicode(pointer);
        }
    }

    public void Delete(string providerProfile)
    {
        if (CredDelete(Target(providerProfile), GenericCredential, 0)) return;
        var error = Marshal.GetLastWin32Error();
        if (error != 1168) throw new Win32Exception(error, "Unable to remove the saved API key.");
    }

    private static string Target(string providerProfile)
    {
        if (string.IsNullOrWhiteSpace(providerProfile) || providerProfile.Any(char.IsControl))
            throw new ArgumentException("Choose a valid provider profile.");
        return $"AutoFix/provider-profile/{providerProfile.Trim()}";
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
        public nint CredentialBlob;
        public int Persist;
        public int AttributeCount;
        public nint Attributes;
        public nint TargetAlias;
        public nint UserName;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CredRead(string target, int type, int reserved, out nint credential);

    [DllImport("advapi32.dll", EntryPoint = "CredWriteW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CredWrite(ref Credential credential, int flags);

    [DllImport("advapi32.dll", EntryPoint = "CredDeleteW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CredDelete(string target, int type, int flags);

    [DllImport("advapi32.dll")]
    private static extern void CredFree(nint credential);
}
