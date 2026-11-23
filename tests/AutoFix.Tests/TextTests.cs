using AutoFix.Core.Text;

namespace AutoFix.Tests;

[TestClass]
public sealed class EditPlannerTests
{
    [TestMethod]
    public void KeepsCommonPrefix()
    {
        var plan = EditPlanner.Plan("teh cat ", "the cat ", "");

        Assert.IsNotNull(plan);
        Assert.AreEqual(7, plan.Backspaces);
        Assert.AreEqual("he cat ", plan.Insert);
    }

    [TestMethod]
    public void RetypesTextTypedAfterTheFix()
    {
        var plan = EditPlanner.Plan("I hav. ", "I have. ", "Next");

        Assert.IsNotNull(plan);
        Assert.AreEqual(2 + 4, plan.Backspaces);
        Assert.AreEqual("e. Next", plan.Insert);
    }

    [TestMethod]
    public void RefusesToBackspaceOverEmoji()
    {
        Assert.IsNull(EditPlanner.Plan("teh 😀 ", "the 😀 ", ""));
    }

    [TestMethod]
    public void RefusesToBackspaceOverCombiningMarks()
    {
        Assert.IsNull(EditPlanner.Plan("cafe teh", "cafe the", "x\u0301"));
    }

    [TestMethod]
    public void AllowsEmojiBeforeTheChange()
    {
        var plan = EditPlanner.Plan("😀 teh", "😀 the", "");

        Assert.IsNotNull(plan);
        Assert.AreEqual(2, plan.Backspaces);
    }

    [TestMethod]
    public void RefusesNewLines()
    {
        Assert.IsNull(EditPlanner.Plan("teh", "the\nend", ""));
    }

    [TestMethod]
    public void HandlesArabic()
    {
        var plan = EditPlanner.Plan("مرحبا بكم فى", "مرحبا بكم في", "");

        Assert.IsNotNull(plan);
        Assert.AreEqual(1, plan.Backspaces);
        Assert.AreEqual("ي", plan.Insert);
    }
}

[TestClass]
public sealed class ResponseGuardTests
{
    [TestMethod]
    public void AcceptsSmallFix() =>
        Assert.AreEqual("I have a problem with the code.",
            ResponseGuard.Clean("I hav a problm wiht teh code.", "I have a problem with the code.", grammar: false));

    [TestMethod]
    public void StripsWrappers()
    {
        Assert.AreEqual("the cat", ResponseGuard.Clean("teh cat", "\"the cat\"", false));
        Assert.AreEqual("the cat", ResponseGuard.Clean("teh cat", "<text>the cat</text>", false));
        Assert.AreEqual("the cat", ResponseGuard.Clean("teh cat", "```\nthe cat\n```", false));
        Assert.AreEqual("the cat", ResponseGuard.Clean("teh cat", "<think>easy</think>\nthe cat", false));
        Assert.AreEqual("the cat", ResponseGuard.Clean("teh cat", "Corrected text: the cat", false));
    }

    [TestMethod]
    public void KeepsQuotesTheWriterTyped() =>
        Assert.AreEqual("\"the cat\"", ResponseGuard.Clean("\"teh cat\"", "\"the cat\"", false));

    [TestMethod]
    public void RejectsAnAnswerToTheText() =>
        Assert.IsNull(ResponseGuard.Clean("what is the capital of france",
            "The capital of France is Paris.", false));

    [TestMethod]
    public void RejectsRewrites() =>
        Assert.IsNull(ResponseGuard.Clean("hey can u send me that file",
            "Hello, could you please send me that file?", false));

    [TestMethod]
    public void RejectsEmptyAnswer() => Assert.IsNull(ResponseGuard.Clean("teh", "  ", false));

    [TestMethod]
    public void SpellingModeKeepsLowercaseStartAndMissingPeriod() =>
        Assert.AreEqual("hey how are you", ResponseGuard.Clean("hey how ar you", "Hey how are you.", false));

    [TestMethod]
    public void SpellingModeAllowsCapitalI() =>
        Assert.AreEqual("I think so", ResponseGuard.Clean("i think so", "I think so", false));

    [TestMethod]
    public void GrammarModeAllowsCapitalization() =>
        Assert.AreEqual("Hey, how are you?", ResponseGuard.Clean("hey how are you", "Hey, how are you?", true));

    [TestMethod]
    public void EditDistanceCountsSwapsOnce()
    {
        Assert.AreEqual(1, ResponseGuard.EditDistance("teh", "the"));
        Assert.AreEqual(3, ResponseGuard.EditDistance("kitten", "sitting"));
    }
}

[TestClass]
public sealed class TextDiffTests
{
    [TestMethod]
    public void ListsChangedWords()
    {
        var changes = TextDiff.ChangedWords("I hav a problm. ", "I have a problem. ");

        CollectionAssert.AreEqual(
            new[] { new WordChange("hav", "have"), new WordChange("problm", "problem") },
            changes.ToArray());
    }

    [TestMethod]
    public void GroupsAdjacentChanges()
    {
        var changes = TextDiff.ChangedWords("alot of", "a lot of");

        CollectionAssert.AreEqual(new[] { new WordChange("alot", "a lot") }, changes.ToArray());
    }

    [TestMethod]
    public void KeepsContractionsAsOneWord()
    {
        var changes = TextDiff.ChangedWords("dont go", "don't go");

        CollectionAssert.AreEqual(new[] { new WordChange("dont", "don't") }, changes.ToArray());
    }

    [TestMethod]
    public void LearnsOnlyWords()
    {
        var words = TextDiff.ChangedOriginalWords("gonna go!", "going to go.");

        CollectionAssert.AreEqual(new[] { "gonna" }, words.ToArray());
    }
}

[TestClass]
public sealed class DeadKeyTests
{
    [TestMethod]
    public void ComposesAccents()
    {
        Assert.AreEqual("é", DeadKeys.Compose('´', "e"));
        Assert.AreEqual("ê", DeadKeys.Compose('^', "e"));
        Assert.AreEqual("ñ", DeadKeys.Compose('~', "n"));
        Assert.AreEqual("ü", DeadKeys.Compose('"', "u"));
    }

    [TestMethod]
    public void SpaceGivesTheAccentItself() => Assert.AreEqual("'", DeadKeys.Compose('\'', " "));

    [TestMethod]
    public void InvalidPairGivesBothCharacters() => Assert.AreEqual("'t", DeadKeys.Compose('\'', "t"));

    [TestMethod]
    public void UnknownDeadKeyIsUnpredictable() => Assert.IsNull(DeadKeys.Compose('x', "e"));
}
