using System.Diagnostics;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using AutoFix.Core.Engine;
using AutoFix.Core.Llm;
using AutoFix.Core.Settings;
using AutoFix.Services;

namespace AutoFix.UI;

public partial class SettingsWindow : Window
{
    private static readonly (int Ms, string Label)[] PauseChoices =
        [(500, "0.5 s"), (800, "0.8 s"), (1000, "1 s"), (1500, "1.5 s"), (2000, "2 s"), (3000, "3 s")];

    private readonly AutoFixService _service;
    private bool _loading;
    private bool _recordingHotkey;
    private int _testVersion;

    internal SettingsWindow(AutoFixService service)
    {
        _service = service;
        InitializeComponent();
        SourceInitialized += (_, _) => Theme.StyleWindow(this);
        ProviderBox.ItemsSource = ProviderCatalog.All;
        PauseBox.ItemsSource = PauseChoices.Select(c => c.Label).ToList();
        VersionText.Text = $"AutoFix {typeof(SettingsWindow).Assembly.GetName().Version?.ToString(3)}";
        Load();
        _service.Changed += OnServiceChanged;
        Closed += (_, _) =>
        {
            _service.Changed -= OnServiceChanged;
            _service.SetTryBoxFocused(false);
        };
        Loaded += (_, _) =>
        {
            if (!_service.IsReady) KeyBox.Focus();
        };
    }

    private AppSettings Settings => _service.Settings;

    private ProviderPreset SelectedProvider => (ProviderPreset?)ProviderBox.SelectedItem ?? ProviderCatalog.Get(null);

    private void Load()
    {
        _loading = true;
        var settings = Settings;
        EnabledSwitch.IsChecked = settings.Enabled;
        ProviderBox.SelectedItem = ProviderCatalog.Get(settings.Provider);
        LoadProvider(settings);
        SpellingMode.IsChecked = settings.Mode == FixMode.Spelling;
        GrammarMode.IsChecked = settings.Mode == FixMode.Grammar;
        SentenceSwitch.IsChecked = settings.FixOnSentenceEnd;
        PauseSwitch.IsChecked = settings.FixOnPause;
        PauseBox.SelectedIndex = Array.FindIndex(PauseChoices, c => c.Ms >= settings.PauseMs) is var i and >= 0 ? i : 2;
        PausePanel.IsEnabled = settings.FixOnPause;
        EnterSwitch.IsChecked = settings.FixBeforeEnter;
        PopupSwitch.IsChecked = settings.ShowPopup;
        StartupSwitch.IsChecked = settings.StartWithWindows;
        HotkeyBox.Text = settings.Hotkey.Replace("+", " + ");
        RefreshLists(settings);
        RefreshStatus();
        _loading = false;
    }

    private void LoadProvider(AppSettings settings)
    {
        var preset = SelectedProvider;
        var saved = settings.ProviderSettingsFor(preset.Id);
        string model = string.IsNullOrWhiteSpace(saved.Model) ? preset.DefaultModel : saved.Model;
        ModelBox.ItemsSource = string.IsNullOrEmpty(preset.DefaultModel) || model == preset.DefaultModel
            ? new[] { model }.Where(m => m.Length > 0).ToList()
            : new List<string> { model, preset.DefaultModel };
        ModelBox.Text = model;
        BaseUrlPanel.Visibility = preset.IsCustom ? Visibility.Visible : Visibility.Collapsed;
        BaseUrlBox.Text = saved.BaseUrl;
        KeyPanel.Visibility = preset.NeedsKey ? Visibility.Visible : Visibility.Collapsed;
        GetKeyLink.Visibility = preset.KeyUrl is null ? Visibility.Collapsed : Visibility.Visible;
        KeyBox.Password = "";
        KeyHint.Text = AutoFixService.HasApiKey(preset.Id)
            ? "A key is saved for this provider. Paste a new key to replace it."
            : "Stored securely in Windows Credential Manager, never in a file.";
        TestResult.Text = "";
    }

    private void RefreshStatus()
    {
        var settings = Settings;
        string text;
        Brush dot;
        if (!settings.Enabled)
        {
            (text, dot) = ("Paused", (Brush)FindResource("SwitchOffBrush"));
        }
        else if (!_service.IsReady)
        {
            (text, dot) = ("Add an API key to start", (Brush)FindResource("DangerBrush"));
        }
        else if (_service.LastError is { } error)
        {
            (text, dot) = (error, (Brush)FindResource("DangerBrush"));
        }
        else
        {
            text = _service.TypicalLatencyMs is { } ms ? $"On · fixes in about {ms} ms" : "On · ready to fix as you type";
            dot = (Brush)FindResource("SuccessBrush");
        }
        StatusText.Text = text;
        StatusDot.Fill = dot;
        EnabledSwitch.Content = settings.Enabled ? "On" : "Off";
    }

    private void RefreshLists(AppSettings settings)
    {
        Chips(AppChips, settings.BlockedApps, app => _service.Update(s => s.BlockedApps.RemoveAll(a => a == app)));
        Chips(WordChips, settings.Dictionary, word => _service.Update(s => s.Dictionary.RemoveAll(w => w == word)));
    }

    private void Chips(WrapPanel panel, IEnumerable<string> items, Action<string> remove)
    {
        panel.Children.Clear();
        foreach (var item in items.OrderBy(i => i, StringComparer.OrdinalIgnoreCase))
        {
            var button = new Button { Style = (Style)FindResource("ChipRemove") };
            AutomationProperties.SetName(button, "Remove " + item);
            button.Click += (_, _) => remove(item);
            var row = new StackPanel { Orientation = Orientation.Horizontal };
            row.Children.Add(new TextBlock
            {
                Text = item,
                FontSize = 12,
                VerticalAlignment = VerticalAlignment.Center,
                Foreground = (Brush)FindResource("TextBrush"),
            });
            row.Children.Add(button);
            panel.Children.Add(new Border { Style = (Style)FindResource("Chip"), Child = row });
        }
    }

    private void OnServiceChanged() => Dispatcher.BeginInvoke(() =>
    {
        _loading = true;
        var settings = Settings;
        EnabledSwitch.IsChecked = settings.Enabled;
        RefreshLists(settings);
        RefreshStatus();
        _loading = false;
    });

    // Provider -----------------------------------------------------------------------------

    private void OnProviderChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_loading) return;
        var id = SelectedProvider.Id;
        _service.Update(s => s.Provider = id);
        _loading = true;
        LoadProvider(Settings);
        _loading = false;
    }

    private void OnModelCommitted(object sender, KeyboardFocusChangedEventArgs e) => SaveModel();

    private void OnModelSelected(object sender, SelectionChangedEventArgs e)
    {
        if (ModelBox.SelectedItem is string model)
        {
            ModelBox.Text = model;
            SaveModel();
        }
    }

    private void SaveModel()
    {
        if (_loading) return;
        string id = SelectedProvider.Id, model = ModelBox.Text.Trim();
        if (Settings.ProviderSettingsFor(id).Model == model) return;
        _service.Update(s => s.ProviderSettingsFor(id).Model = model);
    }

    private void OnBaseUrlCommitted(object sender, KeyboardFocusChangedEventArgs e)
    {
        if (_loading) return;
        string id = SelectedProvider.Id, url = BaseUrlBox.Text.Trim();
        if (Settings.ProviderSettingsFor(id).BaseUrl != url) _service.Update(s => s.ProviderSettingsFor(id).BaseUrl = url);
    }

    private int _lastKeyLength;

    private void OnKeyChanged(object sender, RoutedEventArgs e)
    {
        string key = KeyBox.Password.Trim();
        bool pasted = key.Length - _lastKeyLength > 12;
        _lastKeyLength = key.Length;
        if (key.Length == 0) return;

        if (ProviderCatalog.DetectFromKey(key) is { } detected && detected.Id != SelectedProvider.Id)
        {
            ProviderBox.SelectedItem = detected; // Saves the provider and clears the box.
            KeyBox.Password = key;
            KeyHint.Text = $"This looks like a {detected.Name} key, so the provider was switched.";
            return;
        }
        // Pasting a whole key is the common case: save and test right away.
        if (pasted) OnTest(sender, e);
    }

    private void OnGetKey(object sender, RoutedEventArgs e)
    {
        if (SelectedProvider.KeyUrl is { } url) Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
    }

    private async void OnTest(object sender, RoutedEventArgs e)
    {
        var preset = SelectedProvider;
        string key = KeyBox.Password.Trim();
        SaveModel();
        if (preset.IsCustom) OnBaseUrlCommitted(sender, null!);
        if (preset.NeedsKey && key.Length == 0 && !AutoFixService.HasApiKey(preset.Id))
        {
            ShowTest(false, "Paste your API key first.");
            KeyBox.Focus();
            return;
        }
        if (preset.IsCustom && !Uri.TryCreate(BaseUrlBox.Text.Trim(), UriKind.Absolute, out _))
        {
            ShowTest(false, "Enter the server address, for example https://example.com/v1.");
            return;
        }

        int version = ++_testVersion;
        TestButton.IsEnabled = false;
        ShowTest(null, "Testing…");
        try
        {
            if (key.Length > 0) _service.SaveApiKey(preset.Id, key);
            var result = await _service.TestAsync(preset.Id, ModelBox.Text, BaseUrlBox.Text, null);
            if (version != _testVersion) return;
            ShowTest(result.Ok, result.Ok ? $"✓ {result.Message}. Try it below." : result.Message);
            if (key.Length > 0)
            {
                KeyBox.Password = "";
                _lastKeyLength = 0;
                KeyHint.Text = "A key is saved for this provider. Paste a new key to replace it.";
            }
            if (result.Ok)
            {
                if (!Settings.Onboarded) _service.Update(s => s.Onboarded = true);
                _ = LoadModelsAsync(preset);
            }
        }
        catch (Exception ex)
        {
            ShowTest(false, ex.Message);
        }
        finally
        {
            TestButton.IsEnabled = true;
        }
    }

    private async Task LoadModelsAsync(ProviderPreset preset)
    {
        try
        {
            var models = await _service.ListModelsAsync(preset.Id, BaseUrlBox.Text, null);
            if (models.Count == 0 || SelectedProvider.Id != preset.Id) return;
            string current = ModelBox.Text;
            _loading = true;
            ModelBox.ItemsSource = models.Contains(current) ? models : models.Prepend(current).ToList();
            ModelBox.Text = current;
            _loading = false;
        }
        catch (LlmException)
        {
            // The list is optional; typing a model name always works.
        }
    }

    private void ShowTest(bool? ok, string message)
    {
        TestResult.Text = message;
        TestResult.Foreground = (Brush)FindResource(ok switch
        {
            true => "SuccessBrush",
            false => "DangerBrush",
            null => "SubTextBrush",
        });
        RefreshStatus();
    }

    // Behavior ------------------------------------------------------------------------------

    private void OnEnabledClick(object sender, RoutedEventArgs e)
    {
        bool enabled = EnabledSwitch.IsChecked == true;
        _service.Update(s => s.Enabled = enabled);
    }

    private void OnModeClick(object sender, RoutedEventArgs e)
    {
        var mode = GrammarMode.IsChecked == true ? FixMode.Grammar : FixMode.Spelling;
        _service.Update(s => s.Mode = mode);
    }

    private void OnBehaviorClick(object sender, RoutedEventArgs e)
    {
        if (_loading) return;
        bool sentence = SentenceSwitch.IsChecked == true, pause = PauseSwitch.IsChecked == true,
            enter = EnterSwitch.IsChecked == true, popup = PopupSwitch.IsChecked == true, startup = StartupSwitch.IsChecked == true;
        PausePanel.IsEnabled = pause;
        _service.Update(s =>
        {
            s.FixOnSentenceEnd = sentence;
            s.FixOnPause = pause;
            s.FixBeforeEnter = enter;
            s.ShowPopup = popup;
            s.StartWithWindows = startup;
        });
    }

    private void OnPauseChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_loading || PauseBox.SelectedIndex < 0) return;
        int ms = PauseChoices[PauseBox.SelectedIndex].Ms;
        _service.Update(s => s.PauseMs = ms);
    }

    private void OnTryFocus(object sender, KeyboardFocusChangedEventArgs e) => _service.SetTryBoxFocused(true);

    private void OnTryBlur(object sender, KeyboardFocusChangedEventArgs e) => _service.SetTryBoxFocused(false);

    // Shortcut recorder ---------------------------------------------------------------------

    private void OnHotkeyFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        _recordingHotkey = true;
        HotkeyBox.Text = "Press keys…";
    }

    private void OnHotkeyBlur(object sender, KeyboardFocusChangedEventArgs e)
    {
        _recordingHotkey = false;
        HotkeyBox.Text = Settings.Hotkey.Replace("+", " + ");
    }

    private void OnHotkeyKeyDown(object sender, KeyEventArgs e)
    {
        if (!_recordingHotkey) return;
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (key is Key.Tab) return;
        e.Handled = true;
        if (key is Key.Escape)
        {
            Keyboard.ClearFocus();
            return;
        }
        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift
            or Key.LWin or Key.RWin) return;

        var mods = Mods.None;
        if (Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) mods |= Mods.Ctrl;
        if (Keyboard.Modifiers.HasFlag(ModifierKeys.Alt)) mods |= Mods.Alt;
        if (Keyboard.Modifiers.HasFlag(ModifierKeys.Shift)) mods |= Mods.Shift;
        if (Keyboard.Modifiers.HasFlag(ModifierKeys.Windows)) mods |= Mods.Win;
        var chord = KeyChord.Create(mods, KeyInterop.VirtualKeyFromKey(key));
        if (chord is null)
        {
            HotkeyBox.Text = "Add Ctrl, Alt or Win";
            return;
        }
        string text = chord.ToString();
        _service.Update(s => s.Hotkey = text);
        HotkeyBox.Text = text.Replace("+", " + ");
        _recordingHotkey = false;
        Keyboard.ClearFocus();
    }

    // Apps and words ------------------------------------------------------------------------

    private void OnAddAppOpened(object? sender, EventArgs e)
    {
        var blocked = Settings.BlockedApps.ToHashSet(StringComparer.OrdinalIgnoreCase);
        int self = Environment.ProcessId;
        AddAppBox.ItemsSource = Process.GetProcesses()
            .Where(p =>
            {
                try
                {
                    return p.Id != self && p.MainWindowHandle != IntPtr.Zero;
                }
                catch (InvalidOperationException)
                {
                    return false;
                }
            })
            .Select(p => p.ProcessName + ".exe")
            .Where(name => !blocked.Contains(name))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .Order(StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    private void OnAddAppKey(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter) OnAddApp(sender, e);
    }

    private void OnAddApp(object sender, RoutedEventArgs e)
    {
        string name = AddAppBox.Text.Trim();
        if (name.Length == 0) return;
        if (!name.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)) name += ".exe";
        _service.Update(s =>
        {
            if (!s.IsBlocked(name)) s.BlockedApps.Add(name);
        });
        AddAppBox.Text = "";
    }

    private void OnResetApps(object sender, RoutedEventArgs e) =>
        _service.Update(s => s.BlockedApps = [.. AppSettings.DefaultBlockedApps]);

    private void OnWordKey(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter) OnAddWord(sender, e);
    }

    private void OnAddWord(object sender, RoutedEventArgs e)
    {
        string word = WordBox.Text.Trim();
        if (word.Length == 0) return;
        _service.Update(s =>
        {
            if (!s.Dictionary.Contains(word, StringComparer.OrdinalIgnoreCase)) s.Dictionary.Add(word);
        });
        WordBox.Text = "";
    }

    private void OnOpenLogs(object sender, RoutedEventArgs e)
    {
        if (Log.FilePath is { } path)
            Process.Start(new ProcessStartInfo(Path.GetDirectoryName(path)!) { UseShellExecute = true });
    }
}
