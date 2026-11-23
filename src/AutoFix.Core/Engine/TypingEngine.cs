using System.Text;
using AutoFix.Core.Text;

namespace AutoFix.Core.Engine;

/// <summary>
/// Tracks the text typed before the caret in the focused field, decides when to
/// correct it, and applies results as "backspace then retype" edits.
///
/// The buffer holds only text typed since the last caret move or focus change, so
/// the engine never needs to read the document. User input that arrives while an
/// edit is being typed, or while Enter is held for a fix, is swallowed and
/// replayed afterwards in its original order.
///
/// Not thread-safe: every call must come from the input thread.
/// </summary>
public sealed class TypingEngine
{
    internal const int MaxTargetChars = 1200;
    internal const int ContextChars = 400;
    internal const int InjectTimeoutMs = 1500;
    internal const int DeferRetryMs = 40;
    internal const int DeferMaxMs = 1500;
    internal const int WorkingNoticeDelayMs = 250;
    private const int MaxBufferChars = 6000;
    private const int MaxTriggersPerHold = 2;

    private readonly IEngineHost _host;
    private readonly StringBuilder _buffer = new();
    private readonly List<object> _queue = [];
    private EngineOptions _options;
    private bool _focusAllowed = true;
    private int _epoch;
    private int _checkedUpTo;
    private int _nextId;
    private Pending? _inFlight;
    private Deferred? _deferred;
    private LastFix? _lastFix;
    private bool _injecting;
    private long _injectStartedAt;
    private bool _holding;
    private long _holdStartedAt;
    private int _holdTriggers;
    private bool _workingShown;
    private bool _pauseArmed;
    private long _lastTextAt;
    private int _typingStartedEpoch = -1;

    private sealed record Pending(int Id, int Epoch, int Start, string Original, string Lead, string Trail)
    {
        public int End => Start + Original.Length;
    }

    private sealed record Deferred(Pending Request, string Text, long Deadline);

    private sealed record LastFix(int Epoch, int Start, string Original, string Corrected);

    public TypingEngine(IEngineHost host, EngineOptions options)
    {
        _host = host;
        _options = options;
    }

    /// <summary>Text typed before the caret, as far as the engine knows.</summary>
    public string Text => _buffer.ToString();

    public int CheckedLength => _checkedUpTo;

    public bool IsHolding => _holding;

    public bool IsInjecting => _injecting;

    private bool Active => _options.Enabled && _options.Ready && _focusAllowed;

    private long Now => _host.NowMs;

    public void UpdateOptions(EngineOptions options)
    {
        _options = options;
        if (!Active) Reset();
        ScheduleTick();
    }

    /// <returns>True when the key must be swallowed.</returns>
    public bool OnKey(KeyStroke key)
    {
        if (_holding || _injecting)
        {
            if (key.Raw is null) return false;
            _queue.Add(key.Raw);
            return true;
        }
        if (!key.IsDown || !Active) return false;

        switch (key.Kind)
        {
            case KeyKind.Modifier:
                return false;
            case KeyKind.Neutral:
                _lastFix = null;
                return false;
            case KeyKind.Reset:
                Reset();
                return false;
            case KeyKind.Text:
                OnText(key.Text ?? "");
                return false;
            case KeyKind.Backspace:
                if (key.Mods == Mods.None && TryUndo()) return true;
                _lastFix = null;
                DeleteLastChar();
                return false;
            case KeyKind.Enter:
                if (!key.IsReplay && key.Mods == Mods.None && key.Raw is not null && _options.FixBeforeEnter
                    && (_inFlight is not null || HasUncheckedLetters()))
                {
                    StartHold(key.Raw);
                    return true;
                }
                Reset();
                return false;
            default:
                return false;
        }
    }

    /// <returns>True when the mouse button event must be swallowed.</returns>
    public bool OnMouseButton(object? raw, bool isDown)
    {
        if ((_holding || _injecting) && raw is not null)
        {
            _queue.Add(raw);
            return true;
        }
        if (isDown) Reset();
        return false;
    }

    public void OnFocusChanged(bool allowed)
    {
        Reset();
        _focusAllowed = allowed;
        if (_holding)
        {
            if (_injecting) return;
            ReleaseHold();
        }
    }

    /// <summary>The correction shortcut was pressed.</summary>
    public void OnHotkey()
    {
        if (!_options.Enabled) return;
        if (Active && _buffer.Length > 0 && ContainsLetter(0, _buffer.Length))
        {
            if (_inFlight is not null || _injecting || _holding) return;
            if (!HasUncheckedLetters()) _checkedUpTo = Math.Max(0, _buffer.Length - MaxTargetChars);
            Trigger(CorrectionTrigger.Shortcut);
            ScheduleTick();
            return;
        }
        _host.FixSelection();
    }

    public void OnCorrectionResult(int id, CorrectionOutcome outcome)
    {
        if (_inFlight is null || _inFlight.Id != id) return;
        var request = _inFlight;
        _inFlight = null;
        if (request.Epoch == _epoch)
        {
            switch (outcome.Status)
            {
                case CorrectionStatus.Blocked:
                    Reset();
                    break;
                case CorrectionStatus.Failed:
                    // Do not retry the same text on every pause.
                    if (RegionIntact(request)) _checkedUpTo = Math.Max(_checkedUpTo, request.End);
                    break;
                case CorrectionStatus.Corrected:
                    Apply(request, outcome.Text ?? request.Original.Trim());
                    break;
            }
        }
        AfterResult();
    }

    public void OnInjectionComplete(bool succeeded)
    {
        if (!_injecting) return;
        _injecting = false;
        if (!succeeded) Reset();
        if (!_holding) FlushQueue();
        AfterResult();
    }

    public void OnTick()
    {
        long now = Now;
        if (_injecting && now - _injectStartedAt >= InjectTimeoutMs)
        {
            OnInjectionComplete(false);
        }

        if (_deferred is { } deferred)
        {
            if (!_host.ShortcutModifiersHeld)
            {
                _deferred = null;
                if (deferred.Request.Epoch == _epoch) Apply(deferred.Request, deferred.Text);
                AfterResult();
            }
            else if (now >= deferred.Deadline)
            {
                _deferred = null;
                AfterResult();
            }
        }

        if (_holding)
        {
            if (!_workingShown && now - _holdStartedAt >= WorkingNoticeDelayMs)
            {
                _workingShown = true;
                _host.ShowWorking(true);
            }
            if (now - _holdStartedAt >= _options.EnterHoldMaxMs)
            {
                // Give up waiting: send Enter now and ignore the late answer.
                _inFlight = null;
                _deferred = null;
                if (!_injecting) ReleaseHold();
            }
        }

        if (PauseDue(now) && Idle)
        {
            _pauseArmed = false;
            Trigger(CorrectionTrigger.Pause);
        }
        ScheduleTick();
    }

    public void Reset()
    {
        _buffer.Clear();
        _checkedUpTo = 0;
        _epoch++;
        _inFlight = null;
        _deferred = null;
        _lastFix = null;
        _pauseArmed = false;
    }

    private bool Idle => !_holding && !_injecting && _inFlight is null && _deferred is null;

    private bool PauseDue(long now) =>
        _pauseArmed && _options.FixOnPause && now - _lastTextAt >= _options.PauseMs;

    private void OnText(string text)
    {
        if (text.Length == 0) return;
        _lastFix = null;
        if (_buffer.Length == 0 && _typingStartedEpoch != _epoch)
        {
            _typingStartedEpoch = _epoch;
            _host.TypingStarted(_epoch);
        }
        _buffer.Append(text);
        _lastTextAt = Now;
        _pauseArmed = _options.FixOnPause;
        TrimBuffer();
        if (_options.FixOnSentenceEnd && IsSentenceEnd(_buffer.Length)) Trigger(CorrectionTrigger.SentenceEnd);
        ScheduleTick();
    }

    private void DeleteLastChar()
    {
        if (_buffer.Length == 0) return;
        int remove = _buffer.Length >= 2 && char.IsLowSurrogate(_buffer[^1]) && char.IsHighSurrogate(_buffer[^2]) ? 2 : 1;
        _buffer.Length -= remove;
        _checkedUpTo = Math.Min(_checkedUpTo, _buffer.Length);
    }

    private void Trigger(CorrectionTrigger trigger)
    {
        if (!Active || _inFlight is not null) return;
        int end = trigger == CorrectionTrigger.SentenceEnd ? LastSentenceEnd() : _buffer.Length;
        if (end <= _checkedUpTo) return;

        int start = WordStart(_checkedUpTo);
        if (end - start > MaxTargetChars)
        {
            // Too long for one request: skip the oldest part, starting at a word boundary.
            start = end - MaxTargetChars;
            while (start < end && !char.IsWhiteSpace(_buffer[start - 1])) start++;
            _checkedUpTo = start;
        }

        string original = _buffer.ToString(start, end - start);
        if (!ContainsLetter(start, end))
        {
            _checkedUpTo = end;
            return;
        }

        string core = original.Trim();
        if (core.Length < 2) return;
        string lead = original[..original.IndexOf(core, StringComparison.Ordinal)];
        string trail = original[(lead.Length + core.Length)..];

        _inFlight = new Pending(++_nextId, _epoch, start, original, lead, trail);
        _host.StartCorrection(new CorrectionRequest(_inFlight.Id, core, ContextBefore(start), trigger, _epoch));
    }

    private void Apply(Pending request, string correctedCore)
    {
        if (!RegionIntact(request)) return;
        string suffix = _buffer.ToString(request.End, _buffer.Length - request.End);

        // The user kept typing the same word; this answer is for a word that no longer exists.
        if (suffix.Length > 0 && IsWordChar(request.Original[^1]) && IsWordChar(suffix[0])) return;

        string corrected = request.Lead + correctedCore + request.Trail;
        if (corrected == request.Original)
        {
            _checkedUpTo = Math.Max(_checkedUpTo, request.End);
            return;
        }

        if (_host.ShortcutModifiersHeld)
        {
            // Backspaces would become Ctrl+Backspace; wait for the keys to be released.
            _deferred = new Deferred(request, correctedCore, _deferred?.Deadline ?? Now + DeferMaxMs);
            return;
        }

        var plan = EditPlanner.Plan(request.Original, corrected, suffix);
        if (plan is null)
        {
            _checkedUpTo = Math.Max(_checkedUpTo, request.End);
            return;
        }

        StartInjection(plan);
        _buffer.Length = request.Start;
        _buffer.Append(corrected).Append(suffix);
        _checkedUpTo = request.Start + corrected.Length;
        _lastFix = suffix.Length == 0 ? new LastFix(_epoch, request.Start, request.Original, corrected) : null;
        _host.ShowFix(new FixNotice(request.Original, corrected, _lastFix is not null));
    }

    private bool TryUndo()
    {
        var fix = _lastFix;
        if (fix is null || fix.Epoch != _epoch || _inFlight is not null || _host.ShortcutModifiersHeld) return false;
        if (_buffer.Length != fix.Start + fix.Corrected.Length) return false;
        if (_buffer.ToString(fix.Start, fix.Corrected.Length) != fix.Corrected) return false;

        var plan = EditPlanner.Plan(fix.Corrected, fix.Original, "");
        if (plan is null) return false;

        _lastFix = null;
        StartInjection(plan);
        _buffer.Length = fix.Start;
        _buffer.Append(fix.Original);
        _checkedUpTo = _buffer.Length;
        _pauseArmed = false;
        _host.Learn(TextDiff.ChangedOriginalWords(fix.Original, fix.Corrected));
        return true;
    }

    private void StartInjection(EditPlan plan)
    {
        _injecting = true;
        _injectStartedAt = Now;
        _host.Inject(plan);
    }

    private void StartHold(object raw)
    {
        _holding = true;
        _holdStartedAt = Now;
        _holdTriggers = 0;
        _workingShown = false;
        _queue.Add(raw);
        if (_inFlight is null)
        {
            _holdTriggers++;
            Trigger(CorrectionTrigger.Enter);
        }
        if (_inFlight is null && !_injecting && _deferred is null)
        {
            ReleaseHold();
            return;
        }
        ScheduleTick();
    }

    private void ReleaseHold()
    {
        _holding = false;
        if (_workingShown)
        {
            _workingShown = false;
            _host.ShowWorking(false);
        }
        FlushQueue();
    }

    private void FlushQueue()
    {
        if (_queue.Count == 0) return;
        var inputs = _queue.ToArray();
        _queue.Clear();
        _host.Replay(inputs);
    }

    /// <summary>Starts the next step once the current request or edit is done.</summary>
    private void AfterResult()
    {
        if (_injecting || _deferred is not null || _inFlight is not null)
        {
            ScheduleTick();
            return;
        }

        if (_holding)
        {
            if (_holdTriggers < MaxTriggersPerHold && HasUncheckedLetters())
            {
                _holdTriggers++;
                Trigger(CorrectionTrigger.Enter);
            }
            if (_inFlight is null) ReleaseHold();
            ScheduleTick();
            return;
        }

        if (_options.FixOnSentenceEnd) Trigger(CorrectionTrigger.SentenceEnd);
        if (_inFlight is null && PauseDue(Now))
        {
            _pauseArmed = false;
            Trigger(CorrectionTrigger.Pause);
        }
        ScheduleTick();
    }

    private void ScheduleTick()
    {
        long now = Now;
        long next = long.MaxValue;
        if (_injecting) next = Math.Min(next, _injectStartedAt + InjectTimeoutMs);
        if (_deferred is not null) next = Math.Min(next, now + DeferRetryMs);
        if (_holding)
        {
            if (!_workingShown) next = Math.Min(next, _holdStartedAt + WorkingNoticeDelayMs);
            next = Math.Min(next, _holdStartedAt + _options.EnterHoldMaxMs);
        }
        if (_pauseArmed && _options.FixOnPause) next = Math.Min(next, _lastTextAt + _options.PauseMs);
        _host.RequestTick(next == long.MaxValue ? -1 : (int)Math.Clamp(next - now, 1, int.MaxValue));
    }

    private bool RegionIntact(Pending request) =>
        _buffer.Length >= request.End && _buffer.ToString(request.Start, request.Original.Length) == request.Original;

    private bool HasUncheckedLetters() => ContainsLetter(_checkedUpTo, _buffer.Length);

    private bool ContainsLetter(int start, int end)
    {
        for (int i = start; i < end; i++)
        {
            if (char.IsLetter(_buffer[i])) return true;
        }
        return false;
    }

    /// <summary>End of the last finished sentence after the checked text, or -1.</summary>
    private int LastSentenceEnd()
    {
        for (int end = _buffer.Length; end > _checkedUpTo; end--)
        {
            if (IsSentenceEnd(end)) return end;
        }
        return -1;
    }

    /// <summary>True when the text before <paramref name="end"/> finishes a sentence: "word. " or "word。".</summary>
    private bool IsSentenceEnd(int end)
    {
        if (end < 1) return false;
        char last = _buffer[end - 1];
        if (last is '。' or '！' or '？') return true;
        if (!char.IsWhiteSpace(last)) return false;
        int i = end - 2;
        while (i >= 0 && _buffer[i] is '"' or '\'' or '”' or '’' or ')' or ']' or '»') i--;
        return i >= 0 && _buffer[i] is '.' or '!' or '?' or '…' or '؟';
    }

    /// <summary>Moves back so a request never starts in the middle of a word.</summary>
    private int WordStart(int index)
    {
        index = Math.Clamp(index, 0, _buffer.Length);
        while (index > 0 && index < _buffer.Length && IsWordChar(_buffer[index - 1]) && IsWordChar(_buffer[index])) index--;
        return index;
    }

    private string ContextBefore(int start)
    {
        if (start == 0) return "";
        int from = Math.Max(0, start - ContextChars);
        if (from > 0)
        {
            while (from < start && !char.IsWhiteSpace(_buffer[from])) from++;
        }
        return _buffer.ToString(from, start - from);
    }

    private void TrimBuffer()
    {
        if (_buffer.Length <= MaxBufferChars || _inFlight is not null || _deferred is not null) return;
        int cut = _buffer.Length - MaxBufferChars / 2;
        while (cut < _buffer.Length && !char.IsWhiteSpace(_buffer[cut])) cut++;
        _buffer.Remove(0, cut);
        _checkedUpTo = Math.Max(0, _checkedUpTo - cut);
        _lastFix = null;
    }

    internal static bool IsWordChar(char c) => char.IsLetterOrDigit(c) || c is '\'' or '’' or '-' or '_';
}
