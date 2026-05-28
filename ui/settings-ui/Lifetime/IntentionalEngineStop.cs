namespace AutoFix.SettingsUi.Lifetime;

public sealed class IntentionalEngineStop
{
    private readonly object lockObj = new();
    private ShutdownReason? currentReason;

    public ShutdownReason? CurrentReason
    {
        get
        {
            lock (lockObj)
            {
                return currentReason;
            }
        }
    }

    public bool HasCurrentStop
    {
        get
        {
            lock (lockObj)
            {
                return currentReason is not null;
            }
        }
    }

    public void Mark(ShutdownReason reason)
    {
        lock (lockObj)
        {
            currentReason = reason;
        }
    }

    public bool ConsumeIfMatches(ShutdownReason reason)
    {
        lock (lockObj)
        {
            if (currentReason != reason)
            {
                return false;
            }

            currentReason = null;
            return true;
        }
    }

    public void Clear()
    {
        lock (lockObj)
        {
            currentReason = null;
        }
    }
}
