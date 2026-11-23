using System.Diagnostics;
using System.Runtime.InteropServices;
using AutoFix.Core.Engine;
using AutoFix.Core.Llm;
using AutoFix.Core.Settings;
using AutoFix.Core.Text;
using AutoFix.Input;
using static AutoFix.Native.Win32;

namespace AutoFix.Services;

/// <summary>Shows small on-screen notices. Implementations marshal to the UI thread.</summary>
internal interface INotifier
{
    void ShowFix(IReadOnlyList<WordChange> changes, string hint, RECT? anchor);
    void ShowMessage(string message, RECT? anchor);
    void ShowWorking(bool working, RECT? anchor);
}

public sealed record ConnectionTest(bool Ok, string Message, long LatencyMs = 0);

/// <summary>
/// Connects typing, settings, the LLM and on-screen feedback. Owns the input thread.
/// </summary>
internal sealed class AutoFixService : IInputHost, IDisposable
{
    private static readonly string[] ShellProcesses = ["explorer.exe", "ShellExperienceHost.exe", "StartMenuExperienceHost.exe", "SearchHost.exe"];
    private const string SampleText = "I hav a problm wiht teh code.";

    private readonly SettingsStore _store;
    private readonly LlmClient _client = new();
    private readonly Corrector _corrector;
    private readonly INotifier _notifier;
    private readonly object _gate = new();
    private readonly bool _manageStartup;
    private readonly Dictionary<LlmErrorKind, DateTime> _errorShownAt = [];
    private readonly List<long> _latencies = [];
    private readonly Timer _keepWarm;
    private AppSettings _settings;
    private Snapshot _snapshot;
    private InputThread? _input;
    private long _lastNetworkAt;
    private readonly object _fieldGate = new();
    private (int FieldId, Task<bool> IsPassword) _fieldCheck = (-1, Task.FromResult(false));
    private int _selectionBusy;
    private volatile bool _tryBoxFocused;

    /// <summary>Immutable view of the settings used from background threads.</summary>
    private sealed record Snapshot(
        ProviderConfig Config,
        bool NeedsKey,
        FixMode Mode,
        bool ShowPopup,
        HashSet<string> Blocked,
        IReadOnlyCollection<string> Keep,
        string Hotkey)
    {
        public bool Ready => !string.IsNullOrWhiteSpace(Config.BaseUrl) && !string.IsNullOrWhiteSpace(Config.Model)
            && (!NeedsKey || !string.IsNullOrEmpty(Config.ApiKey));
    }

    public AutoFixService(SettingsStore store, INotifier notifier, bool manageStartup)
    {
        _store = store;
        _notifier = notifier;
        _manageStartup = manageStartup;
        _corrector = new Corrector(_client);
        _settings = store.Load();
        _snapshot = BuildSnapshot(_settings, CredentialStore.Get(_settings.Provider));
        _keepWarm = new Timer(_ => KeepConnectionWarm(), null, Timeout.Infinite, Timeout.Infinite);
    }

    /// <summary>Raised on any thread when settings or status change.</summary>
    public event Action? Changed;

    public AppSettings Settings
    {
        get
        {
            lock (_gate) return _settings.Clone();
        }
    }

    public bool IsReady => _snapshot.Ready;

    public string? LastError { get; private set; }

    public FocusTarget? LastApp { get; private set; }

    public long? TypicalLatencyMs
    {
        get
        {
            lock (_latencies)
            {
                if (_latencies.Count == 0) return null;
                var sorted = _latencies.Order().ToList();
                return sorted[sorted.Count / 2];
            }
        }
    }

    public void Start()
    {
        _input = new InputThread(this, EngineOptionsFor(_settings, _snapshot), KeyChord.TryParse(_settings.Hotkey));
        _input.Start();
        if (_manageStartup) StartupRegistration.Apply(_settings.StartWithWindows);
        WarmUp();
        _keepWarm.Change(TimeSpan.FromSeconds(25), TimeSpan.FromSeconds(25));
        // Load UI Automation once so the first password check is fast.
        Task.Run(() => FocusInspector.IsPasswordField());
        Log.Info($"Started. Provider {_settings.Provider}, model {_snapshot.Config.Model}, ready {IsReady}.");
    }

    public void Update(Action<AppSettings> change)
    {
        bool startup, appsChanged;
        lock (_gate)
        {
            bool before = _settings.StartWithWindows;
            var blockedBefore = _snapshot.Blocked;
            change(_settings);
            startup = before != _settings.StartWithWindows;
            appsChanged = !blockedBefore.SetEquals(_settings.BlockedApps);
            _store.Save(_settings);
            _snapshot = BuildSnapshot(_settings, CredentialStore.Get(_settings.Provider));
            _input?.Update(EngineOptionsFor(_settings, _snapshot), KeyChord.TryParse(_settings.Hotkey));
        }
        if (startup && _manageStartup) StartupRegistration.Apply(_settings.StartWithWindows);
        // Only a changed app list can change whether the focused app may be corrected.
        if (appsChanged) _input?.RefreshFocus();
        WarmUp();
        Changed?.Invoke();
    }

    public void SaveApiKey(string providerId, string? key)
    {
        if (string.IsNullOrWhiteSpace(key)) CredentialStore.Delete(providerId);
        else CredentialStore.Set(providerId, key);
        LastError = null;
        Update(_ => { });
    }

    public static bool HasApiKey(string providerId) => CredentialStore.Get(providerId) is not null;

    /// <summary>Lets the "Try it" box in the settings window be corrected like any other app.</summary>
    public void SetTryBoxFocused(bool focused)
    {
        _tryBoxFocused = focused;
        _input?.RefreshFocus();
    }

    public async Task<ConnectionTest> TestAsync(string providerId, string model, string baseUrl, string? key)
    {
        var config = ConfigFor(providerId, model, baseUrl, key);
        try
        {
            var result = await _corrector.CorrectAsync(config, FixMode.Spelling, SampleText, "", [],
                TimeSpan.FromSeconds(20), CancellationToken.None);
            if (result.Rejected) return new ConnectionTest(false, "The model answered, but not with a correction. Try another model.");
            return result.Changed
                ? new ConnectionTest(true, $"Working · {result.LatencyMs} ms", result.LatencyMs)
                : new ConnectionTest(false, "The model answered but fixed nothing. Try a stronger model.");
        }
        catch (LlmException ex)
        {
            return new ConnectionTest(false, ex.Kind is LlmErrorKind.BadRequest or LlmErrorKind.BadResponse
                ? ex.Message : ex.UserMessage);
        }
    }

    public async Task<IReadOnlyList<string>> ListModelsAsync(string providerId, string baseUrl, string? key)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        return await _client.ListModelsAsync(ConfigFor(providerId, "", baseUrl, key), timeout.Token);
    }

    // IInputHost ----------------------------------------------------------------------------

    public bool OnFocusChanged(FocusTarget target)
    {
        if (!target.IsSelf && target.ProcessName.Length > 0
            && !ShellProcesses.Contains(target.ProcessName, StringComparer.OrdinalIgnoreCase))
        {
            bool changed = LastApp?.ProcessName != target.ProcessName;
            LastApp = target;
            if (changed) Changed?.Invoke();
        }
        return IsAllowed(target);
    }

    private bool IsAllowed(FocusTarget target)
    {
        if (target.Window == IntPtr.Zero || target.Elevated) return false;
        if (target.IsSelf) return _tryBoxFocused;
        return !_snapshot.Blocked.Contains(target.ProcessName);
    }

    public void StartCorrection(CorrectionRequest request, FocusTarget target)
    {
        var snapshot = _snapshot;
        var watch = Stopwatch.StartNew();
        Task.Run(async () =>
        {
            CorrectionOutcome outcome;
            try
            {
                if (request.Trigger != CorrectionTrigger.Shortcut && !LooksLikeProse(request.Text))
                {
                    outcome = CorrectionOutcome.Success(request.Text);
                }
                else if (await FieldCheck(request.FieldId))
                {
                    outcome = CorrectionOutcome.Blocked;
                }
                else
                {
                    Interlocked.Exchange(ref _lastNetworkAt, Environment.TickCount64);
                    var result = await _corrector.CorrectAsync(snapshot.Config, snapshot.Mode, request.Text, request.Context,
                        snapshot.Keep, TimeoutFor(request.Trigger), CancellationToken.None);
                    RecordSuccess(result.LatencyMs);
                    Log.Info($"{request.Trigger}: {request.Text.Length} chars, provider {result.LatencyMs} ms, total {watch.ElapsedMilliseconds} ms, " +
                        (result.Rejected ? "answer rejected" : result.Changed ? "fixed" : "no change"));
                    outcome = CorrectionOutcome.Success(result.Text);
                }
            }
            catch (LlmException ex)
            {
                // Error bodies can echo the request, so the message itself is not logged.
                Log.Warn($"{request.Trigger}: {ex.Kind} (HTTP {ex.Status})");
                ReportError(ex, target);
                outcome = CorrectionOutcome.Fail(ex.Message);
            }
            catch (Exception ex)
            {
                Log.Error("Correction failed", ex);
                outcome = CorrectionOutcome.Fail(ex.Message);
            }
            _input?.Post(engine => engine.OnCorrectionResult(request.Id, outcome));
        });
    }

    public void FixSelection(FocusTarget target)
    {
        var snapshot = _snapshot;
        if (!snapshot.Ready)
        {
            Notify("Add your API key in AutoFix settings first.", target);
            return;
        }
        if (!IsAllowed(target))
        {
            Notify("AutoFix is off in this app.", target);
            return;
        }
        if (Interlocked.Exchange(ref _selectionBusy, 1) == 1) return;
        Task.Run(async () =>
        {
            try
            {
                await FixSelectionAsync(target, snapshot);
            }
            catch (Exception ex)
            {
                Log.Error("Selection fix failed", ex);
            }
            finally
            {
                Volatile.Write(ref _selectionBusy, 0);
            }
        });
    }

    public void ShowFix(FixNotice notice, FocusTarget target)
    {
        if (!_snapshot.ShowPopup) return;
        var changes = TextDiff.ChangedWords(notice.Original, notice.Corrected);
        if (changes.Count == 0) return;
        Task.Run(() => _notifier.ShowFix(changes, notice.CanUndo ? "Backspace to undo" : "", Anchor(target.Window)));
    }

    public void ShowWorking(bool working, FocusTarget target) =>
        Task.Run(() => _notifier.ShowWorking(working, working ? Anchor(target.Window) : null));

    public void Learn(IReadOnlyList<string> words)
    {
        if (words.Count == 0) return;
        // Called from the keyboard hook: save on a worker so the hook returns at once.
        Task.Run(() =>
        {
            Update(settings =>
            {
                foreach (var word in words)
                {
                    if (!settings.Dictionary.Contains(word, StringComparer.OrdinalIgnoreCase)) settings.Dictionary.Add(word);
                }
            });
            Log.Info($"Undo: kept {words.Count} word(s) in the dictionary.");
        });
    }

    // Selection -----------------------------------------------------------------------------

    private async Task FixSelectionAsync(FocusTarget target, Snapshot snapshot)
    {
        // Ctrl+C must not become Ctrl+Alt+C while the shortcut keys are still held.
        var wait = Stopwatch.StartNew();
        while (ModifiersHeld() && wait.ElapsedMilliseconds < 2000) await Task.Delay(15);
        if (ModifiersHeld() || GetForegroundWindow() != target.Window) return;

        if (FocusInspector.IsPasswordField())
        {
            Notify("AutoFix never reads password fields.", target);
            return;
        }
        int inputBefore = _input?.UserInputCount ?? 0;

        var original = ClipboardSnapshot.Capture();
        if (original is null)
        {
            Notify("The clipboard is busy. Try again.", target);
            return;
        }

        string? selected = null;
        uint sequence = GetClipboardSequenceNumber();
        Injector.Send(Injector.BuildCtrlChord(0x43)); // Ctrl+C
        for (int i = 0; i < 50 && selected is null; i++)
        {
            await Task.Delay(10);
            if (GetClipboardSequenceNumber() != sequence) selected = ClipboardSnapshot.GetText() ?? "";
        }
        original.Restore();

        if (string.IsNullOrWhiteSpace(selected))
        {
            Notify($"Select some text, then press {snapshot.Hotkey}.", target);
            return;
        }
        if (selected.Length > 4000)
        {
            Notify("That selection is too long. Select up to 4,000 characters.", target);
            return;
        }

        string core = selected.Trim();
        string lead = selected[..selected.IndexOf(core, StringComparison.Ordinal)];
        string trail = selected[(lead.Length + core.Length)..];
        var anchor = Anchor(target.Window);
        CorrectionResult result;
        _notifier.ShowWorking(true, anchor);
        try
        {
            Interlocked.Exchange(ref _lastNetworkAt, Environment.TickCount64);
            result = await _corrector.CorrectAsync(snapshot.Config, snapshot.Mode, core, "", snapshot.Keep,
                TimeoutFor(CorrectionTrigger.Shortcut), CancellationToken.None);
            RecordSuccess(result.LatencyMs);
            Log.Info($"Selection: {core.Length} chars, {result.LatencyMs} ms, changed {result.Changed}");
        }
        catch (LlmException ex)
        {
            Log.Warn($"Selection: {ex.Kind} (HTTP {ex.Status})");
            _notifier.ShowWorking(false, null);
            Notify(ex.UserMessage, target);
            return;
        }
        _notifier.ShowWorking(false, null);

        if (!result.Changed)
        {
            Notify(result.Rejected ? "AutoFix skipped this text: the answer changed too much." : "Looks good. Nothing to fix.", target);
            return;
        }
        if (GetForegroundWindow() != target.Window || (_input?.UserInputCount ?? 0) != inputBefore)
        {
            Notify("You moved on, so AutoFix did not paste the fix.", target);
            return;
        }

        var current = ClipboardSnapshot.Capture();
        if (!ClipboardSnapshot.SetText(lead + result.Text + trail)) return;
        Injector.Send(Injector.BuildCtrlChord(0x56)); // Ctrl+V
        await Task.Delay(600); // Let the app read the clipboard before restoring it.
        current?.Restore();
        _input?.Post(engine => engine.Reset());

        var changes = TextDiff.ChangedWords(core, result.Text);
        if (_snapshot.ShowPopup && changes.Count > 0) _notifier.ShowFix(changes, "Ctrl+Z to undo", anchor);
    }

    private static bool ModifiersHeld() =>
        IsDown(VK_CONTROL) || IsDown(VK_MENU) || IsDown(VK_SHIFT) || IsDown(VK_LWIN) || IsDown(VK_RWIN);

    // Helpers -------------------------------------------------------------------------------

    /// <summary>
    /// Single tokens such as emails, URLs, codes and passwords are not prose; never send them.
    /// </summary>
    internal static bool LooksLikeProse(string text)
    {
        if (text.Any(char.IsWhiteSpace)) return true;
        if (text.Length > 30) return false;
        return !text.Any(c => char.IsDigit(c) || c is '@' or '/' or '\\' or '_' or '=' or '#' or '$' or '%' or '&' or '*');
    }

    /// <summary>Starts the password-field check as soon as typing begins, so requests do not wait for it.</summary>
    public void PrepareField(int fieldId) => FieldCheck(fieldId);

    private Task<bool> FieldCheck(int fieldId)
    {
        lock (_fieldGate)
        {
            if (_fieldCheck.FieldId != fieldId) _fieldCheck = (fieldId, Task.Run(FocusInspector.IsPasswordField));
            return _fieldCheck.IsPassword;
        }
    }

    private static TimeSpan TimeoutFor(CorrectionTrigger trigger) => trigger switch
    {
        CorrectionTrigger.Enter => TimeSpan.FromSeconds(3),
        CorrectionTrigger.Shortcut => TimeSpan.FromSeconds(15),
        _ => TimeSpan.FromSeconds(10),
    };

    private void RecordSuccess(long latency)
    {
        lock (_latencies)
        {
            _latencies.Add(latency);
            if (_latencies.Count > 20) _latencies.RemoveAt(0);
        }
        if (LastError is not null)
        {
            LastError = null;
            Changed?.Invoke();
        }
    }

    private void ReportError(LlmException error, FocusTarget target)
    {
        LastError = error.UserMessage;
        Changed?.Invoke();
        // Connection problems are often temporary; only show problems the user must fix.
        if (error.Kind is LlmErrorKind.Network or LlmErrorKind.Timeout or LlmErrorKind.Server) return;
        lock (_errorShownAt)
        {
            if (_errorShownAt.TryGetValue(error.Kind, out var shown) && DateTime.UtcNow - shown < TimeSpan.FromMinutes(1)) return;
            _errorShownAt[error.Kind] = DateTime.UtcNow;
        }
        Notify(error.UserMessage, target);
    }

    private void Notify(string message, FocusTarget target) =>
        Task.Run(() => _notifier.ShowMessage(message, Anchor(target.Window)));

    private static RECT? Anchor(IntPtr window)
    {
        if (window == IntPtr.Zero) return null;
        if (FocusInspector.CaretRect(window) is { } caret) return caret;
        // No caret information: center near the bottom of the window.
        if (!GetWindowRect(window, out var rect)) return null;
        int x = (rect.Left + rect.Right) / 2;
        int y = rect.Bottom - 120;
        return new RECT { Left = x, Top = y, Right = x, Bottom = y };
    }

    private void WarmUp()
    {
        var snapshot = _snapshot;
        if (!snapshot.Ready || !_settings.Enabled) return;
        Interlocked.Exchange(ref _lastNetworkAt, Environment.TickCount64);
        _ = _client.WarmUpAsync(snapshot.Config, CancellationToken.None);
    }

    /// <summary>Servers close idle connections; reopen while the user is active so fixes skip the TLS handshake.</summary>
    private void KeepConnectionWarm()
    {
        long now = Environment.TickCount64;
        if (now - Interlocked.Read(ref _lastNetworkAt) < 40_000) return;
        var info = new LASTINPUTINFO { cbSize = (uint)Marshal.SizeOf<LASTINPUTINFO>() };
        if (!GetLastInputInfo(ref info) || (uint)Environment.TickCount - info.dwTime > 90_000) return;
        WarmUp();
    }

    private static ProviderConfig ConfigFor(string providerId, string model, string baseUrl, string? key)
    {
        var preset = ProviderCatalog.Get(providerId);
        return new ProviderConfig(preset.Kind,
            preset.IsCustom ? baseUrl.Trim() : preset.BaseUrl,
            string.IsNullOrWhiteSpace(model) ? preset.DefaultModel : model.Trim(),
            string.IsNullOrWhiteSpace(key) ? CredentialStore.Get(providerId) : key.Trim());
    }

    private static Snapshot BuildSnapshot(AppSettings settings, string? key) => new(
        settings.ProviderConfig(key),
        ProviderCatalog.Get(settings.Provider).NeedsKey,
        settings.Mode,
        settings.ShowPopup,
        new HashSet<string>(settings.BlockedApps, StringComparer.OrdinalIgnoreCase),
        settings.Dictionary.ToArray(),
        settings.Hotkey);

    private static EngineOptions EngineOptionsFor(AppSettings settings, Snapshot snapshot) => new(
        Enabled: settings.Enabled,
        Ready: snapshot.Ready,
        FixOnSentenceEnd: settings.FixOnSentenceEnd,
        FixOnPause: settings.FixOnPause,
        PauseMs: settings.PauseMs,
        FixBeforeEnter: settings.FixBeforeEnter,
        EnterHoldMaxMs: 3000);

    [StructLayout(LayoutKind.Sequential)]
    private struct LASTINPUTINFO
    {
        public uint cbSize;
        public uint dwTime;
    }

    [DllImport("user32.dll")]
    private static extern bool GetLastInputInfo(ref LASTINPUTINFO info);

    public void Dispose()
    {
        _keepWarm.Dispose();
        _input?.Dispose();
        _client.Dispose();
    }
}
