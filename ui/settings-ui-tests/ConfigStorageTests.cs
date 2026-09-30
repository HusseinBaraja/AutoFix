using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class ConfigStorageTests
{
    /// <summary>Saved confidence choices round-trip and imported unsafe low-tier behavior fails validation.</summary>
    [TestMethod]
    public void ConfidenceSettingsRoundTripAndRejectUnsafeLowBehavior()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Correction.HighConfidenceBehavior = "do_nothing";
        config.Correction.MediumConfidenceBehavior = "silent";
        fixture.Storage.Save(config);
        var loaded = fixture.Storage.Load(fixture.Path);
        Assert.AreEqual("do_nothing", loaded.Correction.HighConfidenceBehavior);
        Assert.AreEqual("silent", loaded.Correction.MediumConfidenceBehavior);
        Assert.AreEqual("do_nothing", loaded.Correction.LowConfidenceBehavior);
        foreach (var behavior in new[] { "silent", "suggestion" })
        {
            File.WriteAllText(fixture.Path, File.ReadAllText(fixture.Path).Replace("low_confidence_behavior = \"do_nothing\"", $"low_confidence_behavior = \"{behavior}\""));
            Assert.ThrowsException<InvalidDataException>(() => fixture.Storage.Load(fixture.Path));
            fixture.Storage.Save(config);
        }
    }

    [TestMethod]
    public void ApiDefaultsKeepFallbackOffAndRequireSecureCustomEndpoint()
    {
        var config = AppConfig.Default();
        Assert.IsFalse(config.Api.FallbackToLocal);
        Assert.AreEqual(3000L, config.Api.TimeoutManualMs);
        Assert.AreEqual(700L, config.Api.TimeoutAutoMs);
        config.Api.ProviderPreset = "custom";
        Assert.ThrowsException<InvalidDataException>(() => ConfigValidator.Validate(config));
        config.Api.BaseUrl = "http://example.com/v1";
        Assert.ThrowsException<InvalidDataException>(() => ConfigValidator.Validate(config));
        config.Api.BaseUrl = "http://127.0.0.1:9000/v1";
        ConfigValidator.Validate(config);
    }

    [TestMethod]
    public void SaveWritesTomlWithoutApiKeys()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();

        fixture.Storage.Save(config);

        var text = File.ReadAllText(fixture.Path);
        Assert.IsTrue(text.Contains("[general]"));
        Assert.IsTrue(text.Contains("start_with_windows = false"));
        Assert.IsTrue(text.Contains("correct_arbitrary_selection = false"));
        Assert.IsFalse(text.Contains("api_key"));
    }

    [TestMethod]
    public void ArbitrarySelectionSettingRoundTrips()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Shortcuts.CorrectArbitrarySelection = true;
        fixture.Storage.Save(config);

        Assert.IsTrue(fixture.Storage.Load(fixture.Path).Shortcuts.CorrectArbitrarySelection);
    }

    /// <summary>Language policies survive storage while duplicate process overrides are rejected.</summary>
    [TestMethod]
    public void LanguageSettingsRoundTripAndRejectDuplicateApp()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Correction.PreferredLanguage = "en-US";
        config.Correction.AppLanguageOverrides = ["notepad.exe=fr-FR"];
        config.Correction.UncertainLanguagePolicy = "do_nothing";
        config.Correction.MixedLanguagePolicy = "disable_correction";
        fixture.Storage.Save(config);
        var loaded = fixture.Storage.Load(fixture.Path);
        Assert.AreEqual("en-US", loaded.Correction.PreferredLanguage);
        CollectionAssert.AreEqual(config.Correction.AppLanguageOverrides, loaded.Correction.AppLanguageOverrides);
        Assert.AreEqual("do_nothing", loaded.Correction.UncertainLanguagePolicy);
        Assert.AreEqual("disable_correction", loaded.Correction.MixedLanguagePolicy);
        loaded.Correction.AppLanguageOverrides.Add("NOTEPAD.EXE=de");
        Assert.ThrowsException<InvalidDataException>(() => ConfigValidator.Validate(loaded));
    }

    /// <summary>Per-token policy is rejected for local correction and accepted for the API engine.</summary>
    [TestMethod]
    public void PerTokenPolicyRequiresApiEngine()
    {
        var config = AppConfig.Default();
        config.Correction.MixedLanguagePolicy = "per_token";
        Assert.ThrowsException<InvalidDataException>(() => ConfigValidator.Validate(config));
        config.Correction.Engine = "api";
        ConfigValidator.Validate(config);
    }

    /// <summary>Both language settings accept complete tags and reject malformed tags before saving.</summary>
    [TestMethod]
    public void LanguageTagsMatchSharedCasesAndInvalidSavesPreserveFile()
    {
        using var fixture = TempConfigFixture.Create();
        fixture.Storage.Save(AppConfig.Default());
        var original = File.ReadAllText(fixture.Path);
        var cases = System.Text.Json.JsonSerializer.Deserialize<System.Text.Json.JsonElement[]>(
            File.ReadAllText(System.IO.Path.Combine(AppContext.BaseDirectory, "language-tag-cases.json")))!;
        foreach (var item in cases)
        {
            var tag = item[0].GetString()!;
            var valid = item[1].GetBoolean();
            foreach (var appOverride in new[] { false, true })
            {
                var config = AppConfig.Default();
                var field = appOverride ? "correction.app_language_overrides" : "correction.preferred_language";
                if (appOverride)
                {
                    config.Correction.AppLanguageOverrides = [$"notepad.exe={tag}"];
                }
                else
                {
                    config.Correction.PreferredLanguage = tag;
                }
                if (valid)
                {
                    fixture.Storage.Save(config);
                    var loaded = fixture.Storage.Load(fixture.Path);
                    if (appOverride)
                    {
                        CollectionAssert.AreEqual(config.Correction.AppLanguageOverrides, loaded.Correction.AppLanguageOverrides);
                    }
                    else
                    {
                        Assert.AreEqual(tag, loaded.Correction.PreferredLanguage);
                    }
                    fixture.Storage.Save(AppConfig.Default());
                }
                else
                {
                    // App overrides intentionally trim the tag around '='.
                    if (appOverride && tag.Trim() != tag)
                    {
                        continue;
                    }
                    var error = Assert.ThrowsException<InvalidDataException>(() => fixture.Storage.Save(config), tag);
                    StringAssert.Contains(error.Message, field);
                    Assert.AreEqual(original, File.ReadAllText(fixture.Path), tag);
                }
            }
        }
    }

    /// <summary>Legacy punctuation config normalizes to the supported spacing category on load.</summary>
    [TestMethod]
    public void LegacyPunctuationCategoryLoadsAsSpacing()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        config.Correction.Mode = "typos_plus_grammar";
        config.Correction.EnabledGrammarCategories = ["spacing"];
        fixture.Storage.Save(config);
        File.WriteAllText(fixture.Path, File.ReadAllText(fixture.Path).Replace("\"spacing\"", "\"punctuation\""));

        CollectionAssert.AreEqual(new[] { "spacing" }, fixture.Storage.Load(fixture.Path).Correction.EnabledGrammarCategories);
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
            model = "gemini-2.5-flash-lite"
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
        Assert.IsFalse(config.Onboarding.Completed);
        Assert.IsTrue(config.Correction.Enabled);
        Assert.IsFalse(config.Shortcuts.CorrectArbitrarySelection);
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
