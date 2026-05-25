namespace AutoFix.SettingsUi.Lifetime;

public sealed class IntentionalEngineStop
{
    public ShutdownReason? CurrentReason { get; private set; }

    public bool HasCurrentStop => CurrentReason is not null;

    public void Mark(ShutdownReason reason) => CurrentReason = reason;

    public bool ConsumeIfMatches(ShutdownReason reason)
    {
        if (CurrentReason != reason)
        {
            return false;
        }

        CurrentReason = null;
        return true;
    }

    public void Clear() => CurrentReason = null;
}
