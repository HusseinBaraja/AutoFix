namespace AutoFix.Core.Engine;

[Flags]
public enum Mods
{
    None = 0,
    Shift = 1,
    Ctrl = 2,
    Alt = 4,
    Win = 8,
}

/// <summary>What a key press means for the text before the caret.</summary>
public enum KeyKind
{
    /// <summary>Inserts <see cref="KeyStroke.Text"/> at the caret.</summary>
    Text,
    /// <summary>Deletes one character before the caret.</summary>
    Backspace,
    Enter,
    /// <summary>Shift, Ctrl, Caps Lock and similar keys. Text is unchanged.</summary>
    Modifier,
    /// <summary>Keys that do not touch text before the caret (Delete, volume keys).</summary>
    Neutral,
    /// <summary>Keys that may move the caret or change text in unknown ways.</summary>
    Reset,
}

/// <param name="Raw">Platform event, kept so a swallowed key can be replayed in order.</param>
public readonly record struct KeyStroke(
    KeyKind Kind,
    bool IsDown,
    string? Text = null,
    Mods Mods = Mods.None,
    bool IsReplay = false,
    object? Raw = null);

public enum CorrectionTrigger
{
    SentenceEnd,
    Pause,
    Enter,
    Shortcut,
}

/// <param name="FieldId">Changes whenever the caret may have moved to another field.</param>
public sealed record CorrectionRequest(int Id, string Text, string Context, CorrectionTrigger Trigger, int FieldId);

public enum CorrectionStatus
{
    Corrected,
    Failed,
    /// <summary>The focused field must not be corrected, for example a password box.</summary>
    Blocked,
}

public sealed record CorrectionOutcome(CorrectionStatus Status, string? Text = null, string? Error = null)
{
    public static CorrectionOutcome Success(string text) => new(CorrectionStatus.Corrected, text);
    public static CorrectionOutcome Fail(string error) => new(CorrectionStatus.Failed, Error: error);
    public static readonly CorrectionOutcome Blocked = new(CorrectionStatus.Blocked);
}

/// <summary>Delete <see cref="Backspaces"/> characters before the caret, then type <see cref="Insert"/>.</summary>
public sealed record EditPlan(int Backspaces, string Insert);

public sealed record FixNotice(string Original, string Corrected, bool CanUndo);

public sealed record EngineOptions(
    bool Enabled = true,
    bool Ready = true,
    bool FixOnSentenceEnd = true,
    bool FixOnPause = true,
    int PauseMs = 1000,
    bool FixBeforeEnter = true,
    int EnterHoldMaxMs = 3000);

/// <summary>Platform services used by <see cref="TypingEngine"/>. All calls happen on the input thread.</summary>
public interface IEngineHost
{
    long NowMs { get; }

    /// <summary>True when Ctrl, Alt or Win is physically held, so typed keys would become shortcuts.</summary>
    bool ShortcutModifiersHeld { get; }

    /// <summary>Calls <see cref="TypingEngine.OnTick"/> after the delay. A new request replaces the previous one.</summary>
    void RequestTick(int delayMs);

    /// <summary>Typing began in a field; a good moment to prepare checks the first request needs.</summary>
    void TypingStarted(int fieldId);

    /// <summary>Starts a request; the result arrives through <see cref="TypingEngine.OnCorrectionResult"/>.</summary>
    void StartCorrection(CorrectionRequest request);

    /// <summary>Queues the edit. Completion arrives through <see cref="TypingEngine.OnInjectionComplete"/>.</summary>
    void Inject(EditPlan plan);

    /// <summary>Sends swallowed user input again, in its original order.</summary>
    void Replay(IReadOnlyList<object> inputs);

    /// <summary>Corrects the current selection (shortcut pressed with no tracked typing).</summary>
    void FixSelection();

    void ShowFix(FixNotice notice);

    void ShowWorking(bool working);

    /// <summary>The user undid a fix; these original words should be left alone from now on.</summary>
    void Learn(IReadOnlyList<string> words);
}
