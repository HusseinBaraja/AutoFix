using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;

namespace AutoFix.SettingsUi.Settings;

/// <summary>Coordinates native/UI settings access and flushes replacements before completing an import.</summary>
internal static class ConfigFileAccess
{
    internal static FileStream Acquire(string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        return new FileStream(path + ".lock", FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
    }

    internal static void WriteDurable(string path, byte[] bytes)
    {
        using var file = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.None);
        file.Write(bytes);
        file.Flush(flushToDisk: true);
    }

    internal static void ReplaceDurable(string source, string destination)
    {
        if (!MoveFileEx(source, destination, 1 | 8))
            throw new IOException("Unable to replace settings durably.", new Win32Exception(Marshal.GetLastWin32Error()));
    }

    internal static string DatabasePath(string configPath) => Path.Combine(Path.GetDirectoryName(Path.GetFullPath(configPath))!, "autofix.sqlite");

    [DllImport("kernel32.dll", EntryPoint = "MoveFileExW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool MoveFileEx(string source, string destination, uint flags);
}
