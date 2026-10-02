using AutoFix.SettingsUi.Models;
using AutoFix.SettingsUi.Settings;
using AutoFix.SettingsUi.ViewModels;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class DictionaryStorageTests
{
    [TestMethod]
    public void WordsPhrasesScopesAndPairsRoundTripAndRemainUnique()
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new DictionaryStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        storage.Save(new DictionaryItem { Word = "teh phrase", Language = "en", Source = "dictionary" });
        storage.Save(new DictionaryItem { Word = "teh phrase", Language = "EN", Source = "dictionary" });
        storage.Save(new DictionaryItem { Word = "teh", Language = "en-US", App = "Notepad.EXE", Source = "pair", Replacement = "the" });
        var rows = storage.List();
        Assert.AreEqual(2, rows.Count);
        var pair = rows.Single(e => e.Source == "pair");
        Assert.AreEqual("notepad.exe", pair.App);
        Assert.AreEqual("the", pair.Replacement);
        Assert.AreEqual("en-US", pair.Language);
        storage.Save(new DictionaryItem { Word = "wierd", Language = "und", Source = "dictionary" }, pair);
        rows = storage.List();
        Assert.AreEqual(2, rows.Count);
        Assert.IsTrue(rows.All(e => e.Source == "dictionary"));
        storage.Delete(rows.Single(e => e.Word == "wierd"));
        Assert.AreEqual(1, storage.List().Count);
    }

    [TestMethod]
    public void InvalidEditPreservesExistingEntry()
    {
        using var fixture = TempConfigFixture.Create();
        var storage = new DictionaryStorage(Path.Combine(fixture.Root, "autofix.sqlite"));
        storage.Save(new DictionaryItem { Word = "teh", Language = "en", Source = "dictionary" });
        var row = storage.List().Single();
        foreach (var bad in new[] {
            new DictionaryItem { Word = "", Language = "en", Source = "dictionary" },
            new DictionaryItem { Word = "teh", Language = "English-US-!", Source = "dictionary" },
            new DictionaryItem { Word = "teh", Language = "en", Source = "dictionary", App = @"C:\app.exe" },
            new DictionaryItem { Word = "teh", Language = "en", Source = "pair", Replacement = "teh" } })
        {
            Assert.ThrowsException<ArgumentException>(() => storage.Save(bad, row));
            Assert.AreEqual("teh", storage.List().Single().Word);
        }
    }

    [TestMethod]
    public void EmptyDictionaryEditorStaysVisibleAndLearningDefaultsOff()
    {
        var section = SettingsSkeleton.CreateSections().Single(s => s.ShowsDictionary);
        Assert.IsTrue(section.HasDictionary);
        Assert.AreEqual(0, section.Dictionary.Count);
        Assert.AreEqual("off", section.Settings.Single(s => s.Path == "learning.mode").SelectedValue);
        Assert.AreEqual("pair", section.Settings.Single(s => s.Path == "learning.rule").SelectedValue);
    }

    [TestMethod]
    public void LearningChoicesPersistAndLegacyConfigDefaultsOff()
    {
        using var fixture = TempConfigFixture.Create();
        var config = AppConfig.Default();
        foreach (var mode in new[] { "off", "ask", "automatic" })
        foreach (var rule in new[] { "dictionary", "pair" })
        {
            config.Learning.Mode = mode; config.Learning.Rule = rule; config.Learning.PerApp = true;
            var mapped = ConfigFormMapper.BuildConfig(SettingsSkeleton.CreateSections(config));
            fixture.Storage.Save(mapped);
            var loaded = fixture.Storage.Load(fixture.Path);
            Assert.AreEqual(mode, loaded.Learning.Mode);
            Assert.AreEqual(rule, loaded.Learning.Rule);
            Assert.IsTrue(loaded.Learning.PerApp);
        }
        var toml = File.ReadAllText(fixture.Path);
        var start = toml.IndexOf("[learning]", StringComparison.Ordinal);
        var end = toml.IndexOf('[', start + 1);
        File.WriteAllText(fixture.Path, end < 0 ? toml[..start] : toml.Remove(start, end-start));
        Assert.AreEqual("off", fixture.Storage.Load(fixture.Path).Learning.Mode);
        config.Learning.Mode = "unknown";
        Assert.ThrowsException<System.IO.InvalidDataException>(() => ConfigValidator.Validate(config));
    }
}
