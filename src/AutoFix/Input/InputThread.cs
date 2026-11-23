using System.Collections.Concurrent;
using System.Diagnostics;
using System.Runtime.InteropServices;
using AutoFix.Core.Engine;
using AutoFix.Core.Settings;
using AutoFix.Services;
using static AutoFix.Native.Win32;

namespace AutoFix.Input;

/// <summary>Services the input thread needs from the rest of the app. Called on the input thread.</summary>
internal interface IInputHost
{
    /// <returns>True when AutoFix may track and correct typing in this window.</returns>
    bool OnFocusChanged(FocusTarget target);
    void StartCorrection(CorrectionRequest request, FocusTarget target);
    void PrepareField(int fieldId);
    void FixSelection(FocusTarget target);
    void ShowFix(FixNotice notice, FocusTarget target);
    void ShowWorking(bool working, FocusTarget target);
    void Learn(IReadOnlyList<string> words);
}

/// <summary>
/// A dedicated thread that owns the keyboard and mouse hooks and the <see cref="TypingEngine"/>.
/// Hook callbacks must return within a few milliseconds, so this thread never does network,
/// UI Automation or disk work; it only updates state and calls <c>SendInput</c>.
/// </summary>
internal sealed unsafe class InputThread : IEngineHost, IDisposable
{
    private const uint WM_INVOKE = WM_APP + 1;

    private readonly IInputHost _host;
    private readonly Thread _thread;
    private readonly ConcurrentQueue<Action> _actions = new();
    private readonly ManualResetEventSlim _started = new();
    private readonly Stopwatch _clock = Stopwatch.StartNew();
    private readonly KeyTranslator _keys = new();

    // SendInput on the hook thread waits for this thread's own hook on every event (about 1 ms each).
    // A separate sender thread lets the hook handle events as they stream through.
    private readonly System.Collections.Concurrent.BlockingCollection<Action> _sender = new();
    private TypingEngine _engine = null!;
    private EngineOptions _options;
    private KeyChord? _hotkey;
    private int _hotkeyUpToSwallow;
    private uint _threadId;
    private UIntPtr _timer;
    private int _expectedEditEvents;
    private FocusTarget _focus = FocusTarget.None;
    private (IntPtr Window, int Object, int Child) _focusKey;
    private Exception? _startError;

    // Delegates must stay referenced while the hooks are installed.
    private HookProc? _keyboardProc;
    private HookProc? _mouseProc;
    private WinEventProc? _winEventProc;
    private IntPtr _keyboardHook;
    private IntPtr _mouseHook;
    private IntPtr _foregroundHook;
    private IntPtr _focusHook;

    public InputThread(IInputHost host, EngineOptions options, KeyChord? hotkey)
    {
        _host = host;
        _options = options;
        _hotkey = hotkey;
        _thread = new Thread(Run) { IsBackground = true, Name = "AutoFix input" };
        _thread.SetApartmentState(ApartmentState.STA);
        _thread.Priority = ThreadPriority.AboveNormal;
    }

    public void Start()
    {
        new Thread(() =>
        {
            foreach (var send in _sender.GetConsumingEnumerable()) Safe(send);
        }) { IsBackground = true, Name = "AutoFix sender" }.Start();
        _thread.Start();
        _started.Wait();
        if (_startError is not null) throw new InvalidOperationException("Keyboard hook failed to start.", _startError);
    }

    /// <summary>Runs an action on the input thread.</summary>
    public void Post(Action action)
    {
        _actions.Enqueue(action);
        PostThreadMessage(_threadId, WM_INVOKE, UIntPtr.Zero, IntPtr.Zero);
    }

    public void Post(Action<TypingEngine> action) => Post(() => action(_engine));

    public void Update(EngineOptions options, KeyChord? hotkey) => Post(() =>
    {
        _options = options;
        _hotkey = hotkey;
        _engine.UpdateOptions(options);
    });

    /// <summary>Re-reads the focused window, for example after focus moves inside AutoFix's own window.</summary>
    public void RefreshFocus() => Post(() => SetFocus(GetForegroundWindow()));

    // IEngineHost ---------------------------------------------------------------------------

    public long NowMs => _clock.ElapsedMilliseconds;

    public bool ShortcutModifiersHeld => IsDown(VK_CONTROL) || IsDown(VK_MENU) || IsDown(VK_LWIN) || IsDown(VK_RWIN);

    public void RequestTick(int delayMs)
    {
        if (delayMs < 0)
        {
            if (_timer != UIntPtr.Zero) KillTimer(IntPtr.Zero, _timer);
            _timer = UIntPtr.Zero;
            return;
        }
        // Passing the existing id replaces that timer.
        _timer = SetTimer(IntPtr.Zero, _timer, (uint)Math.Max(1, delayMs), IntPtr.Zero);
    }

    public void StartCorrection(CorrectionRequest request) => _host.StartCorrection(request, _focus);

    public void TypingStarted(int fieldId) => _host.PrepareField(fieldId);

    public void Inject(EditPlan plan)
    {
        var inputs = Injector.BuildEdit(plan);
        var target = _focus.Window;
        _expectedEditEvents = inputs.Length;
        _sender.Add(() =>
        {
            if (GetForegroundWindow() == target && Injector.Send(inputs)) return;
            Log.Warn("Could not type the fix: the window changed or Windows refused the input.");
            Post(() =>
            {
                _expectedEditEvents = 0;
                _engine.OnInjectionComplete(false);
            });
        });
    }

    public void Replay(IReadOnlyList<object> inputs)
    {
        var batch = Injector.BuildReplay(inputs);
        _sender.Add(() =>
        {
            if (!Injector.Send(batch)) Log.Warn("SendInput refused replayed input.");
        });
    }

    public void FixSelection() => _host.FixSelection(_focus);

    public void ShowFix(FixNotice notice) => _host.ShowFix(notice, _focus);

    public void ShowWorking(bool working) => _host.ShowWorking(working, _focus);

    public void Learn(IReadOnlyList<string> words) => _host.Learn(words);

    // Thread --------------------------------------------------------------------------------

    private void Run()
    {
        try
        {
            _threadId = GetCurrentThreadId();
            PeekMessage(out _, IntPtr.Zero, 0, 0, 0); // Create the message queue.
            _engine = new TypingEngine(this, _options);
            _keyboardProc = KeyboardProc;
            _mouseProc = MouseProc;
            _winEventProc = WinEventProc;
            IntPtr module = GetModuleHandle(null);
            _keyboardHook = SetWindowsHookEx(WH_KEYBOARD_LL, _keyboardProc, module, 0);
            _mouseHook = SetWindowsHookEx(WH_MOUSE_LL, _mouseProc, module, 0);
            if (_keyboardHook == IntPtr.Zero || _mouseHook == IntPtr.Zero)
                throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            _foregroundHook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, IntPtr.Zero,
                _winEventProc, 0, 0, WINEVENT_OUTOFCONTEXT);
            _focusHook = SetWinEventHook(EVENT_OBJECT_FOCUS, EVENT_OBJECT_FOCUS, IntPtr.Zero,
                _winEventProc, 0, 0, WINEVENT_OUTOFCONTEXT);
            SetFocus(GetForegroundWindow());
        }
        catch (Exception ex)
        {
            _startError = ex;
            _started.Set();
            return;
        }
        _started.Set();

        while (GetMessage(out MSG msg, IntPtr.Zero, 0, 0) > 0)
        {
            if (msg.hwnd == IntPtr.Zero && msg.message == WM_INVOKE)
            {
                while (_actions.TryDequeue(out var action)) Safe(action);
            }
            else if (msg.hwnd == IntPtr.Zero && msg.message == WM_TIMER && msg.wParam == _timer)
            {
                KillTimer(IntPtr.Zero, _timer);
                _timer = UIntPtr.Zero;
                Safe(_engine.OnTick);
            }
            else
            {
                TranslateMessage(ref msg);
                DispatchMessage(ref msg);
            }
        }

        UnhookWindowsHookEx(_keyboardHook);
        UnhookWindowsHookEx(_mouseHook);
        UnhookWinEvent(_foregroundHook);
        UnhookWinEvent(_focusHook);
    }

    private static void Safe(Action action)
    {
        try
        {
            action();
        }
        catch (Exception ex)
        {
            Log.Error("Input thread action failed", ex);
        }
    }

    private IntPtr KeyboardProc(int code, IntPtr message, IntPtr data)
    {
        if (code >= 0)
        {
            try
            {
                if (OnKeyboard(ref *(KBDLLHOOKSTRUCT*)data)) return 1;
            }
            catch (Exception ex)
            {
                Log.Error("Keyboard hook failed", ex);
            }
        }
        return CallNextHookEx(IntPtr.Zero, code, message, data);
    }

    private bool OnKeyboard(ref KBDLLHOOKSTRUCT data)
    {
        if (data.dwExtraInfo == Injector.EditMarker)
        {
            if (_expectedEditEvents > 0 && --_expectedEditEvents == 0) _engine.OnInjectionComplete(true);
            return false;
        }
        if (data.dwExtraInfo == Injector.CommandMarker) return false;

        bool replay = data.dwExtraInfo == Injector.ReplayMarker;
        var raw = new RawKey((ushort)data.vkCode, (ushort)data.scanCode, (data.flags & LLKHF_UP) != 0,
            (data.flags & LLKHF_EXTENDED) != 0);

        if (!replay && _hotkey is { } hotkey)
        {
            if (!raw.Up && raw.Vk == hotkey.VirtualKey && _keys.Mods == hotkey.Mods)
            {
                _hotkeyUpToSwallow = raw.Vk;
                _engine.OnHotkey();
                return true;
            }
            if (raw.Up && raw.Vk == _hotkeyUpToSwallow)
            {
                _hotkeyUpToSwallow = 0;
                return true;
            }
        }

        var stroke = _keys.Translate(raw, replay, KeyboardLayout());
        bool swallow = _engine.OnKey(stroke);
        if (!swallow) _keys.Update(raw);
        return swallow;
    }

    private IntPtr MouseProc(int code, IntPtr message, IntPtr data)
    {
        if (code >= 0)
        {
            int msg = (int)message;
            if (msg is WM_LBUTTONDOWN or WM_LBUTTONUP or WM_RBUTTONDOWN or WM_RBUTTONUP or WM_MBUTTONDOWN
                or WM_MBUTTONUP or WM_XBUTTONDOWN or WM_XBUTTONUP)
            {
                try
                {
                    ref var info = ref *(MSLLHOOKSTRUCT*)data;
                    if (info.dwExtraInfo != Injector.CommandMarker)
                    {
                        bool replay = info.dwExtraInfo == Injector.ReplayMarker;
                        var raw = replay ? null : new RawMouse(msg, info.pt.X, info.pt.Y, info.mouseData);
                        bool down = msg is WM_LBUTTONDOWN or WM_RBUTTONDOWN or WM_MBUTTONDOWN or WM_XBUTTONDOWN;
                        if (down) _keys.Resync();
                        if (_engine.OnMouseButton(raw, down)) return 1;
                    }
                }
                catch (Exception ex)
                {
                    Log.Error("Mouse hook failed", ex);
                }
            }
        }
        return CallNextHookEx(IntPtr.Zero, code, message, data);
    }

    private void WinEventProc(IntPtr hook, uint eventType, IntPtr hwnd, int idObject, int idChild, uint thread, uint time)
    {
        try
        {
            if (eventType == EVENT_SYSTEM_FOREGROUND)
            {
                SetFocus(hwnd);
            }
            else if (eventType == EVENT_OBJECT_FOCUS)
            {
                var key = (hwnd, idObject, idChild);
                if (key == _focusKey) return;
                _focusKey = key;
                SetFocus(GetForegroundWindow());
            }
        }
        catch (Exception ex)
        {
            Log.Error("Focus tracking failed", ex);
        }
    }

    /// <summary>Any focus change may move the caret to another field, so typed text is forgotten.</summary>
    private void SetFocus(IntPtr window)
    {
        _focus = FocusInspector.Describe(window);
        _engine.OnFocusChanged(_host.OnFocusChanged(_focus));
    }

    private static IntPtr KeyboardLayout()
    {
        uint thread = GetWindowThreadProcessId(GetForegroundWindow(), out _);
        return GetKeyboardLayout(thread);
    }

    [DllImport("user32.dll")]
    private static extern bool PeekMessage(out MSG msg, IntPtr hwnd, uint min, uint max, uint remove);

    public void Dispose()
    {
        _sender.CompleteAdding();
        if (_threadId != 0) PostThreadMessage(_threadId, WM_QUIT, UIntPtr.Zero, IntPtr.Zero);
        _thread.Join(TimeSpan.FromSeconds(2));
    }
}
