using System.Diagnostics;
using System.Net;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

// End-to-end check of the real AutoFix.exe: a mock OpenAI-compatible server answers corrections,
// and this harness types into its own Win32 text boxes with real key events.
// Usage: AutoFix.E2E <path to AutoFix.exe>
// It types only while its own window is in the foreground and stops otherwise.

static class E2E
{
    static readonly Dictionary<string, string> Fixes = new(StringComparer.Ordinal)
    {
        ["hav"] = "have", ["problm"] = "problem", ["wiht"] = "with", ["teh"] = "the", ["Teh"] = "The",
        ["helo"] = "hello", ["wrld"] = "world", ["recieve"] = "receive", ["tommorow"] = "tomorrow",
    };

    static int _requests;
    static int _delayMs = 300;
    static readonly List<string> Seen = [];
    static Form _form = null!;
    static TextBox _box = null!;
    static TextBox _password = null!;
    static int _failures;
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    static long _requestAt, _responseAt;

    [STAThread]
    static int Main(string[] args)
    {
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        string exe = Path.GetFullPath(args.Length > 0 ? args[0] : @"src\AutoFix\bin\Debug\net8.0-windows\AutoFix.exe");
        int port = 18000 + Random.Shared.Next(1000);
        using var listener = new HttpListener();
        listener.Prefixes.Add($"http://127.0.0.1:{port}/");
        listener.Start();
        _ = Task.Run(() => Serve(listener));

        string data = Path.Combine(Path.GetTempPath(), "autofix-e2e-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(data);
        File.WriteAllText(Path.Combine(data, "settings.json"), JsonSerializer.Serialize(new
        {
            enabled = true,
            provider = "custom",
            providers = new Dictionary<string, object> { ["custom"] = new { model = "mock", baseUrl = $"http://127.0.0.1:{port}/v1" } },
            fixOnSentenceEnd = true,
            fixOnPause = false,
            fixBeforeEnter = true,
            showPopup = true,
            startWithWindows = false,
            onboarded = true,
        }));

        var start = new ProcessStartInfo(exe, "--background") { UseShellExecute = false };
        start.Environment["AUTOFIX_DATA_DIR"] = data;
        start.Environment["AUTOFIX_API_KEY"] = "test-key";
        using var app = Process.Start(start)!;
        Thread.Sleep(2000);

        var ui = new Thread(() =>
        {
            _form = new Form { Text = "AutoFix E2E", Width = 700, Height = 360, StartPosition = FormStartPosition.CenterScreen, TopMost = true };
            _box = new TextBox { Multiline = true, Dock = DockStyle.Top, Height = 220, Font = new Font("Segoe UI", 12) };
            _password = new TextBox { UseSystemPasswordChar = true, Dock = DockStyle.Bottom, Font = new Font("Segoe UI", 12) };
            _form.Controls.Add(_box);
            _form.Controls.Add(_password);
            _form.Shown += (_, _) => _form.Activate();
            Application.Run(_form);
        });
        ui.SetApartmentState(ApartmentState.STA);
        ui.Start();
        Thread.Sleep(800);

        try
        {
            Focus(_box);
            Run("sentence is fixed", () =>
            {
                for (int round = 0; round < 5; round++)
                {
                    Clear(_box);
                    Type("I hav a problm wiht teh code.");
                    long lastKey = Clock.ElapsedMilliseconds;
                    Key(0x20);
                    Expect(() => Text(_box) == "I have a problem with the code. ", 3000);
                    long done = Clock.ElapsedMilliseconds;
                    if (round == 0 && Environment.GetEnvironmentVariable("E2E_SHOT") is { } shot)
                    {
                        Thread.Sleep(400);
                        var area = Rectangle.Empty;
                        _form.Invoke(() => area = _form.Bounds);
                        area.Inflate(80, 80);
                        using var image = new Bitmap(area.Width, area.Height);
                        using (var g = Graphics.FromImage(image)) g.CopyFromScreen(area.Location, Point.Empty, area.Size);
                        image.Save(shot);
                    }
                    Console.WriteLine($"    last key to request {_requestAt - lastKey} ms, response to text on screen {done - _responseAt} ms, " +
                        $"total {done - lastKey} ms (mock delay {_delayMs} ms)");
                }
            });

            Run("typing during the request is kept", () =>
            {
                Clear(_box);
                _delayMs = 700;
                Type("Teh cat sat. Then we lef");
                Expect(() => Text(_box) == "The cat sat. Then we lef", 3000);
                _delayMs = 300;
            });

            Run("backspace right after a fix undoes it", () =>
            {
                Clear(_box);
                Type("I hav it. ");
                Expect(() => Text(_box) == "I have it. ", 3000);
                Thread.Sleep(150);
                Key(0x08);
                Expect(() => Text(_box) == "I hav it. ", 2000);
            });

            Run("enter waits for the fix", () =>
            {
                Clear(_box);
                Type("helo wrld\nok");
                Expect(() => Text(_box) == "hello world\r\nok", 3000);
            });

            Run("selection is fixed and clipboard kept", () =>
            {
                Clear(_box);
                _form.Invoke(() =>
                {
                    _box.Text = "we recieve it tommorow";
                    _box.SelectAll();
                    Clipboard.SetText("keep me");
                });
                Chord([0x11, 0x12], 0x20); // Ctrl+Alt+Space
                Expect(() => Text(_box) == "we receive it tomorrow", 4000);
                Thread.Sleep(700);
                string clip = "";
                _form.Invoke(() => clip = Clipboard.GetText());
                if (clip != "keep me") throw new Exception($"clipboard changed to '{clip}'");
            });

            Run("password fields are never sent", () =>
            {
                Focus(_password);
                int before = _requests;
                Type("teh secret. ");
                Thread.Sleep(1500);
                if (_requests != before) throw new Exception("a request was sent from a password field");
            });
        }
        catch (Exception ex)
        {
            Console.WriteLine("ABORTED: " + ex.Message);
            _failures++;
        }
        finally
        {
            try
            {
                app.Kill();
            }
            catch (InvalidOperationException)
            {
            }
            _form.Invoke(() => _form.Close());
            Console.WriteLine($"Requests: {_requests}");
            string log = Path.Combine(data, "autofix.log");
            if (File.Exists(log)) Console.WriteLine("--- app log ---\n" + File.ReadAllText(log));
        }
        Console.WriteLine(_failures == 0 ? "ALL PASSED" : $"{_failures} FAILED");
        return _failures == 0 ? 0 : 1;
    }

    static void Run(string name, Action test)
    {
        try
        {
            test();
            Console.WriteLine("PASS " + name);
        }
        catch (Exception ex)
        {
            _failures++;
            Console.WriteLine($"FAIL {name}: {ex.Message}");
        }
    }

    static void Expect(Func<bool> condition, int timeoutMs)
    {
        var watch = Stopwatch.StartNew();
        while (watch.ElapsedMilliseconds < timeoutMs)
        {
            if (condition()) return;
            Thread.Sleep(10);
        }
        throw new Exception($"text is '{Text(_box).Replace("\r\n", "\\n")}'");
    }

    static string Text(TextBox box)
    {
        string text = "";
        _form.Invoke(() => text = box.Text);
        return text;
    }

    static void Clear(TextBox box)
    {
        _form.Invoke(() => box.Clear());
        Key(0x23); // End: tells AutoFix the caret moved.
        Thread.Sleep(100);
    }

    static void Focus(TextBox box)
    {
        _form.Invoke(() =>
        {
            _form.Activate();
            box.Focus();
        });
        Thread.Sleep(300);
        if (GetForegroundWindow() != _form.Handle) throw new Exception("could not focus the test window");
    }

    static void Type(string text)
    {
        foreach (char c in text)
        {
            if (c == '\n')
            {
                Key(0x0D);
                continue;
            }
            short scan = VkKeyScan(c);
            int vk = scan & 0xFF;
            bool shift = (scan & 0x100) != 0;
            if (shift) Send(0x10, false);
            Key(vk);
            if (shift) Send(0x10, true);
            Thread.Sleep(15);
        }
    }

    static void Chord(int[] modifiers, int vk)
    {
        foreach (var m in modifiers) Send(m, false);
        Key(vk);
        foreach (var m in modifiers.Reverse()) Send(m, true);
    }

    static void Key(int vk)
    {
        Send(vk, false);
        Send(vk, true);
    }

    static void Send(int vk, bool up)
    {
        if (GetForegroundWindow() != _form.Handle) throw new Exception("test window lost focus; stopped typing");
        var input = new INPUT { type = 1, ki = new KEYBDINPUT { wVk = (ushort)vk, wScan = (ushort)MapVirtualKey((uint)vk, 0), dwFlags = up ? 2u : 0u } };
        SendInput(1, [input], Marshal.SizeOf<INPUT>());
    }

    static async Task Serve(HttpListener listener)
    {
        while (listener.IsListening)
        {
            var context = await listener.GetContextAsync();
            _ = Task.Run(async () =>
            {
                if (context.Request.HttpMethod != "POST")
                {
                    context.Response.StatusCode = 200;
                    context.Response.Close();
                    return;
                }
                string body = await new StreamReader(context.Request.InputStream).ReadToEndAsync();
                string user = (string)JsonNode.Parse(body)!["messages"]![1]!["content"]!;
                string text = Regex.Match(user, "<text>(.*)</text>", RegexOptions.Singleline).Groups[1].Value;
                Interlocked.Increment(ref _requests);
                _requestAt = Clock.ElapsedMilliseconds;
                lock (Seen) Seen.Add(text);
                string fixedText = Regex.Replace(text, @"\b\w+\b", m => Fixes.GetValueOrDefault(m.Value, m.Value));
                await Task.Delay(_delayMs);
                var answer = new JsonObject
                {
                    ["choices"] = new JsonArray { new JsonObject { ["message"] = new JsonObject { ["content"] = fixedText } } },
                };
                byte[] bytes = Encoding.UTF8.GetBytes(answer.ToJsonString());
                context.Response.ContentType = "application/json";
                await context.Response.OutputStream.WriteAsync(bytes);
                context.Response.Close();
                _responseAt = Clock.ElapsedMilliseconds;
            });
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public UIntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Explicit, Size = 40)]
    struct INPUT
    {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public KEYBDINPUT ki;
    }

    [DllImport("user32.dll")] static extern uint SendInput(uint count, INPUT[] inputs, int size);
    [DllImport("user32.dll")] static extern short VkKeyScan(char c);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint type);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
}
