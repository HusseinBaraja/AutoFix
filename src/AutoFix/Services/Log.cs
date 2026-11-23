namespace AutoFix.Services;

/// <summary>
/// Small local log for troubleshooting. It records timings and errors only, never typed text.
/// </summary>
internal static class Log
{
    private const long MaxBytes = 1_000_000;
    private static readonly object Gate = new();
    private static string? _path;

    public static string? FilePath => _path;

    public static void Initialize(string directory)
    {
        Directory.CreateDirectory(directory);
        _path = Path.Combine(directory, "autofix.log");
    }

    public static void Info(string message) => Write("INFO", message);

    public static void Warn(string message) => Write("WARN", message);

    public static void Error(string message, Exception? error = null) =>
        Write("ERROR", error is null ? message : $"{message}: {error.GetType().Name}: {error.Message}");

    private static void Write(string level, string message)
    {
        if (_path is null) return;
        try
        {
            lock (Gate)
            {
                var file = new FileInfo(_path);
                if (file.Exists && file.Length > MaxBytes) File.Move(_path, _path + ".old", overwrite: true);
                File.AppendAllText(_path, $"{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff} {level} {message}{Environment.NewLine}");
            }
        }
        catch (IOException)
        {
            // Logging must never break the app.
        }
    }
}
