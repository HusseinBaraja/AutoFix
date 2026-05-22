using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ConfigStorageTests
{
    [TestMethod]
    public void SaveWritesTomlWithoutApiKeys()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();

        fixture.Storage.Save(config);

        var text = File.ReadAllText(fixture.Path);
        Assert.IsTrue(text.Contains("[general]"));
        Assert.IsTrue(text.Contains("start_with_windows = false"));
        Assert.IsFalse(text.Contains("api_key"));
    }

    [TestMethod]
    public void LoadReadsCurrentSettings()
    {
        using var fixture = TempConfigFixture.Create();
        File.WriteAllText(
            fixture.Path,
            """
            [general]
            start_with_windows = true
            run_mode = "allowlist"

            [shortcuts]
            correct = "Ctrl+Alt+Space"
            undo = "Ctrl+Alt+Z"

            [triggers]
            word_count_enabled = true
            word_count = 12
            character_trigger_enabled = true
            characters = ["."]

            [context]
            initial_context_words = 25
            initial_context_boundary_chars = ["."]
            forward_movement_word_limit = 5
            informative_context_max_chars = 2000
            informative_context_min_words = 25
            executable_context_max_words = 80

            [correction]
            mode = "typos_only"
            engine = "local"
            high_confidence_behavior = "silent"
            medium_confidence_behavior = "suggestion"
            low_confidence_behavior = "do_nothing"
            enabled_grammar_categories = []

            [api]
            provider_preset = "openai_compatible"
            model = "gpt-4.1-mini"
            timeout_manual_ms = 3000
            timeout_auto_ms = 700
            retry_count = 1
            fallback_to_local = true
            temperature = 0.0
            streaming = false

            [feedback]
            tray_state_enabled = true
            show_correction_applied_notification = false
            show_skipped_reason = true
            show_medium_confidence_suggestions = true
            show_blocked_app_notice = true
            show_timeout_notice = true

            [logging]
            metadata_only_logs_enabled = true
            debug_mode_enabled = false
            redacted_debug_mode_enabled = false
            full_text_debug_mode_enabled = false
            """);

        var config = fixture.Storage.Load(fixture.Path);

        Assert.IsTrue(config.General.StartWithWindows);
        Assert.AreEqual("allowlist", config.General.RunMode);
        Assert.AreEqual(12, config.Triggers.WordCount);
    }

    [TestMethod]
    public void ExportCreatesDestinationDirectory()
    {
        using var fixture = TempConfigFixture.Create();
        var destinationPath = System.IO.Path.Combine(fixture.Root, "exports", "settings.toml");

        fixture.Storage.Export(destinationPath, AppConfig.Default());

        Assert.IsTrue(File.Exists(destinationPath));
    }

}
