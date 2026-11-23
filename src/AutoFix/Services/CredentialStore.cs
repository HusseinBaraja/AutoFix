using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

namespace AutoFix.Services;

/// <summary>
/// Stores API keys in Windows Credential Manager, one entry per provider.
/// Keys never go to the settings file or the log.
/// </summary>
internal static class CredentialStore
{
    private const int GenericCredential = 1;
    private const int PersistLocalMachine = 2;
    private const int NotFound = 1168;

    /// <summary>Set to use a key without storing it (for automated tests).</summary>
    private static readonly string? OverrideKey = Environment.GetEnvironmentVariable("AUTOFIX_API_KEY");

    public static string? Get(string providerId)
    {
        if (!string.IsNullOrEmpty(OverrideKey)) return OverrideKey;
        // Earlier AutoFix versions stored keys under a different name.
        return Read(Target(providerId)) ?? Read("AutoFix/provider-profile/" + providerId);
    }

    public static void Set(string providerId, string key)
    {
        byte[] secret = Encoding.UTF8.GetBytes(key.Trim());
        var handle = GCHandle.Alloc(secret, GCHandleType.Pinned);
        try
        {
            var credential = new CREDENTIAL
            {
                Type = GenericCredential,
                TargetName = Target(providerId),
                UserName = "AutoFix",
                CredentialBlobSize = secret.Length,
                CredentialBlob = handle.AddrOfPinnedObject(),
                Persist = PersistLocalMachine,
            };
            if (!CredWrite(ref credential, 0))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Windows could not save the API key.");
        }
        finally
        {
            Array.Clear(secret);
            handle.Free();
        }
    }

    public static void Delete(string providerId)
    {
        if (!CredDelete(Target(providerId), GenericCredential, 0) && Marshal.GetLastWin32Error() != NotFound)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Windows could not remove the API key.");
    }

    private static string Target(string providerId) => "AutoFix:provider:" + providerId;

    private static string? Read(string target)
    {
        if (!CredRead(target, GenericCredential, 0, out IntPtr pointer)) return null;
        try
        {
            var credential = Marshal.PtrToStructure<CREDENTIAL>(pointer);
            if (credential.CredentialBlobSize <= 0) return null;
            var bytes = new byte[credential.CredentialBlobSize];
            Marshal.Copy(credential.CredentialBlob, bytes, 0, bytes.Length);
            string key = Encoding.UTF8.GetString(bytes).Trim();
            Array.Clear(bytes);
            return key.Length == 0 ? null : key;
        }
        finally
        {
            CredFree(pointer);
        }
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct CREDENTIAL
    {
        public int Flags;
        public int Type;
        public string TargetName;
        public string? Comment;
        public long LastWritten;
        public int CredentialBlobSize;
        public IntPtr CredentialBlob;
        public int Persist;
        public int AttributeCount;
        public IntPtr Attributes;
        public string? TargetAlias;
        public string UserName;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredRead(string target, int type, int flags, out IntPtr credential);

    [DllImport("advapi32.dll", EntryPoint = "CredWriteW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredWrite(ref CREDENTIAL credential, int flags);

    [DllImport("advapi32.dll", EntryPoint = "CredDeleteW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredDelete(string target, int type, int flags);

    [DllImport("advapi32.dll")]
    private static extern void CredFree(IntPtr buffer);
}
