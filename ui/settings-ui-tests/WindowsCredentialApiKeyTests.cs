using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security;
using System.Text;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class WindowsCredentialApiKeyTests
{
    [TestMethod]
    public void KeysRoundTripAsEngineCompatibleUtf8InIsolatedProfile()
    {
        var profile = $"settings-test-{Guid.NewGuid():N}";
        var store = new WindowsCredentialApiKeyStatus();
        var config = AppConfig.Default(); config.Api.ProviderPreset = profile;
        try
        {
            Assert.IsFalse(store.HasConfiguredApiKey(config));
            foreach (var text in new[] { "dummy-key", "updated-ключ" })
            {
                using var key = new SecureString();
                foreach (var c in text) key.AppendChar(c);
                store.Save(profile, key);
                Assert.IsTrue(store.HasConfiguredApiKey(config));
                Assert.IsTrue(CredRead($"AutoFix/provider-profile/{profile}", 1, 0, out var pointer));
                try
                {
                    var credential = Marshal.PtrToStructure<Credential>(pointer);
                    var bytes = new byte[credential.Size];
                    Marshal.Copy(credential.Blob, bytes, 0, bytes.Length);
                    CollectionAssert.AreEqual(Encoding.UTF8.GetBytes(text), bytes);
                    Array.Clear(bytes);
                }
                finally { CredFree(pointer); }
            }
            store.Delete(profile);
            store.Delete(profile);
            Assert.IsFalse(store.HasConfiguredApiKey(config));
        }
        catch (Win32Exception error) when (error.NativeErrorCode == 1312)
        { Assert.Inconclusive("Windows Credential Manager requires a logon session."); }
        finally
        {
            try { store.Delete(profile); }
            catch (Win32Exception error) when (error.NativeErrorCode == 1312) { }
        }
    }

    [TestMethod]
    public void RejectsEmptyAndOversizedKeysWithoutRevealingSecrets()
    {
        var store = new WindowsCredentialApiKeyStatus();
        using var empty = new SecureString();
        Assert.ThrowsException<ArgumentException>(() => store.Save("custom", empty));
        using var oversized = new SecureString();
        for (var i = 0; i < 2561; i++) oversized.AppendChar('x');
        var error = Assert.ThrowsException<ArgumentException>(() => store.Save("custom", oversized));
        Assert.IsFalse(error.Message.Contains(new string('x', 100)));
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Credential
    {
        public int Flags, Type;
        public nint Target, Comment;
        public long LastWritten;
        public int Size;
        public nint Blob;
    }

    [DllImport("advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CredRead(string target, int type, int reserved, out nint credential);
    [DllImport("advapi32.dll")]
    private static extern void CredFree(nint credential);
}
