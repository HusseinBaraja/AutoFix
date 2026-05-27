namespace AutoFix.SettingsUi.Lifetime;

public sealed class RestartPolicy
{
    public const int DefaultMaxAttempts = 3;
    public static readonly TimeSpan DefaultWindow = TimeSpan.FromSeconds(30);

    private readonly int maxAttempts;
    private readonly TimeSpan window;
    private readonly TimeProvider timeProvider;
    private readonly Queue<DateTimeOffset> attempts = new();

    public RestartPolicy(int maxAttempts = DefaultMaxAttempts, TimeSpan? window = null, TimeProvider? timeProvider = null)
    {
        if (maxAttempts < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(maxAttempts));
        }

        if (window.HasValue && window.Value <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(window));
        }

        this.maxAttempts = maxAttempts;
        this.window = window ?? DefaultWindow;
        this.timeProvider = timeProvider ?? TimeProvider.System;
    }

    public bool TryRecordAttempt()
    {
        lock (attempts)
        {
            var now = timeProvider.GetUtcNow();
            while (attempts.Count > 0 && now - attempts.Peek() > window)
            {
                attempts.Dequeue();
            }

            if (attempts.Count >= maxAttempts)
            {
                return false;
            }

            attempts.Enqueue(now);
            return true;
        }
    }

    public void Clear()
    {
        lock (attempts)
        {
            attempts.Clear();
        }
    }
}
