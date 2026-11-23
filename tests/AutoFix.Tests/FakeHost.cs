using AutoFix.Core.Engine;

namespace AutoFix.Tests;

/// <summary>
/// Simulates the focused text field and the platform. Injected edits change
/// <see cref="Screen"/> the way a real editor would.
/// </summary>
internal sealed class FakeHost : IEngineHost
{
    public long NowMs { get; set; }
    public bool ShortcutModifiersHeld { get; set; }
    public int TickDelay { get; private set; } = -1;
    public List<CorrectionRequest> Requests { get; } = [];
    public List<EditPlan> Injected { get; } = [];
    public List<object> Replayed { get; } = [];
    public List<FixNotice> Notices { get; } = [];
    public List<string> Learned { get; } = [];
    public List<bool> Working { get; } = [];
    public int SelectionFixes { get; private set; }
    public string Screen { get; set; } = "";

    public List<int> TypingStarts { get; } = [];
    public void RequestTick(int delayMs) => TickDelay = delayMs;
    public void TypingStarted(int fieldId) => TypingStarts.Add(fieldId);
    public void StartCorrection(CorrectionRequest request) => Requests.Add(request);

    public void Inject(EditPlan plan)
    {
        Injected.Add(plan);
        Screen = Screen[..^plan.Backspaces] + plan.Insert;
    }

    public void Replay(IReadOnlyList<object> inputs) => Replayed.AddRange(inputs);
    public void FixSelection() => SelectionFixes++;
    public void ShowFix(FixNotice notice) => Notices.Add(notice);
    public void ShowWorking(bool working) => Working.Add(working);
    public void Learn(IReadOnlyList<string> words) => Learned.AddRange(words);
}

internal static class EngineDriver
{
    public static (TypingEngine Engine, FakeHost Host) Create(EngineOptions? options = null)
    {
        var host = new FakeHost();
        var engine = new TypingEngine(host, options ?? new EngineOptions());
        return (engine, host);
    }

    /// <summary>Types text the way the keyboard hook reports it, one character per key.</summary>
    public static void Type(this TypingEngine engine, FakeHost host, string text)
    {
        foreach (char c in text)
        {
            bool swallowed = engine.OnKey(new KeyStroke(KeyKind.Text, true, c.ToString(), Raw: "key:" + c));
            if (!swallowed) host.Screen += c;
        }
    }

    public static bool Backspace(this TypingEngine engine, FakeHost host)
    {
        bool swallowed = engine.OnKey(new KeyStroke(KeyKind.Backspace, true, Raw: "key:BS"));
        if (!swallowed && host.Screen.Length > 0) host.Screen = host.Screen[..^1];
        return swallowed;
    }

    public static bool Enter(this TypingEngine engine, Mods mods = Mods.None, bool replay = false) =>
        engine.OnKey(new KeyStroke(KeyKind.Enter, true, Mods: mods, IsReplay: replay, Raw: "key:Enter"));

    public static void Wait(this TypingEngine engine, FakeHost host, int ms)
    {
        host.NowMs += ms;
        engine.OnTick();
    }

    public static void Answer(this TypingEngine engine, FakeHost host, string text)
    {
        engine.OnCorrectionResult(host.Requests[^1].Id, CorrectionOutcome.Success(text));
    }

    /// <summary>Simulates the hook seeing all injected events.</summary>
    public static void FinishInjection(this TypingEngine engine) => engine.OnInjectionComplete(true);
}
