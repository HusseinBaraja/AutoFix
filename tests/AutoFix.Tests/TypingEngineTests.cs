using AutoFix.Core.Engine;

namespace AutoFix.Tests;

[TestClass]
public sealed class TypingEngineTests
{
    private static readonly EngineOptions SentenceOnly = new(FixOnPause: false, FixBeforeEnter: false);

    [TestMethod]
    public void FinishedSentenceIsSentWithoutTrailingSpace()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);

        engine.Type(host, "I hav a problm. ");

        Assert.AreEqual(1, host.Requests.Count);
        Assert.AreEqual("I hav a problm.", host.Requests[0].Text);
        Assert.AreEqual(CorrectionTrigger.SentenceEnd, host.Requests[0].Trigger);
        Assert.AreEqual("", host.Requests[0].Context);
    }

    [TestMethod]
    public void UnfinishedSentenceIsNotSent()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);

        engine.Type(host, "Version 2.5 is out");

        Assert.AreEqual(0, host.Requests.Count);
    }

    [TestMethod]
    public void CorrectionRetypesOnlyFromFirstChange()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "I hav a problm. ");

        engine.Answer(host, "I have a problem.");

        Assert.AreEqual(1, host.Injected.Count);
        Assert.AreEqual(11, host.Injected[0].Backspaces);
        Assert.AreEqual("e a problem. ", host.Injected[0].Insert);
        Assert.AreEqual("I have a problem. ", host.Screen);
        Assert.AreEqual(host.Screen, engine.Text);
        Assert.AreEqual(1, host.Notices.Count);
        Assert.IsTrue(host.Notices[0].CanUndo);
    }

    [TestMethod]
    public void TextTypedWhileWaitingIsKept()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "I hav a problm. ");
        engine.Type(host, "Next");

        engine.Answer(host, "I have a problem.");

        Assert.AreEqual("I have a problem. Next", host.Screen);
        Assert.AreEqual(host.Screen, engine.Text);
        Assert.IsFalse(host.Notices[0].CanUndo);
    }

    [TestMethod]
    public void KeysTypedDuringAnEditAreReplayedAfterIt()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");
        engine.Answer(host, "the cat.");

        bool swallowed = engine.OnKey(new KeyStroke(KeyKind.Text, true, "x", Raw: "key:x"));
        bool mouseSwallowed = engine.OnMouseButton("mouse", true);
        engine.FinishInjection();

        Assert.IsTrue(swallowed);
        Assert.IsTrue(mouseSwallowed);
        CollectionAssert.AreEqual(new object[] { "key:x", "mouse" }, host.Replayed);
    }

    [TestMethod]
    public void NextSentenceGetsPreviousTextAsContext()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "I hav a problm. ");
        engine.Answer(host, "I have a problem.");
        engine.FinishInjection();

        engine.Type(host, "It is wierd. ");

        Assert.AreEqual(2, host.Requests.Count);
        Assert.AreEqual("It is wierd.", host.Requests[1].Text);
        Assert.AreEqual("I have a problem. ", host.Requests[1].Context);
    }

    [TestMethod]
    public void SecondSentenceFinishedDuringRequestIsSentAfterIt()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "Frist one. ");
        engine.Type(host, "Secnd one. ");
        Assert.AreEqual(1, host.Requests.Count);

        engine.Answer(host, "First one.");
        engine.FinishInjection();

        Assert.AreEqual(2, host.Requests.Count);
        Assert.AreEqual("Secnd one.", host.Requests[1].Text);
        Assert.AreEqual("First one. Secnd one. ", host.Screen);
    }

    [TestMethod]
    public void UnchangedAnswerMarksTextChecked()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "All good. ");

        engine.Answer(host, "All good.");

        Assert.AreEqual(0, host.Injected.Count);
        Assert.AreEqual(10, engine.CheckedLength);
    }

    [TestMethod]
    public void AnswerIsDroppedWhenTheUserKeepsTypingTheSameWord()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(FixBeforeEnter: false));
        engine.Type(host, "helo wor");
        engine.Wait(host, 1000);
        Assert.AreEqual("helo wor", host.Requests[0].Text);

        engine.Type(host, "ld");
        engine.Answer(host, "hello wor");

        Assert.AreEqual(0, host.Injected.Count);
        Assert.AreEqual("helo world", host.Screen);
    }

    [TestMethod]
    public void AnswerIsDroppedAfterCaretMoves()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");

        engine.OnKey(new KeyStroke(KeyKind.Reset, true));
        engine.Answer(host, "the cat.");

        Assert.AreEqual(0, host.Injected.Count);
        Assert.AreEqual("", engine.Text);
    }

    [TestMethod]
    public void AnswerIsDroppedWhenUserDeletedIntoTheSentence()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");

        engine.Backspace(host);
        engine.Backspace(host);
        engine.Answer(host, "the cat.");

        Assert.AreEqual(0, host.Injected.Count);
        Assert.AreEqual("teh cat", host.Screen);
    }

    [TestMethod]
    public void BackspaceRightAfterFixUndoesItAndLearns()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "I hav a problm. ");
        engine.Answer(host, "I have a problem.");
        engine.FinishInjection();

        bool swallowed = engine.Backspace(host);

        Assert.IsTrue(swallowed);
        Assert.AreEqual("I hav a problm. ", host.Screen);
        Assert.AreEqual(host.Screen, engine.Text);
        CollectionAssert.AreEqual(new[] { "hav", "problm" }, host.Learned);
        Assert.AreEqual(engine.Text.Length, engine.CheckedLength);
    }

    [TestMethod]
    public void BackspaceAfterMoreTypingIsANormalBackspace()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");
        engine.Answer(host, "the cat.");
        engine.FinishInjection();
        engine.Type(host, "A");

        bool swallowed = engine.Backspace(host);

        Assert.IsFalse(swallowed);
        Assert.AreEqual("the cat. ", host.Screen);
        Assert.AreEqual(0, host.Learned.Count);
    }

    [TestMethod]
    public void EnterWaitsForTheFixThenReplaysKeys()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(FixOnPause: false));
        engine.Type(host, "helo wrld");

        Assert.IsTrue(engine.Enter());
        Assert.IsTrue(engine.IsHolding);
        Assert.AreEqual(CorrectionTrigger.Enter, host.Requests[0].Trigger);
        engine.Type(host, "x");
        Assert.AreEqual("helo wrld", host.Screen);

        engine.Answer(host, "hello world");
        Assert.AreEqual("hello world", host.Screen);
        engine.FinishInjection();

        Assert.IsFalse(engine.IsHolding);
        CollectionAssert.AreEqual(new object[] { "key:Enter", "key:x" }, host.Replayed);
    }

    [TestMethod]
    public void EnterIsReleasedWhenProviderIsSlow()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(FixOnPause: false, EnterHoldMaxMs: 3000));
        engine.Type(host, "helo wrld");
        engine.Enter();

        engine.Wait(host, 300);
        CollectionAssert.AreEqual(new[] { true }, host.Working);
        engine.Wait(host, 2700);

        Assert.IsFalse(engine.IsHolding);
        CollectionAssert.AreEqual(new object[] { "key:Enter" }, host.Replayed);
        CollectionAssert.AreEqual(new[] { true, false }, host.Working);

        engine.Answer(host, "hello world");
        Assert.AreEqual(0, host.Injected.Count);
    }

    [TestMethod]
    public void ReplayedEnterIsNotHeldAgain()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(FixOnPause: false));
        engine.Type(host, "helo wrld");

        Assert.IsFalse(engine.Enter(replay: true));
        Assert.AreEqual(0, host.Requests.Count);
    }

    [TestMethod]
    public void ShiftEnterIsNotHeld()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(FixOnPause: false));
        engine.Type(host, "helo wrld");

        Assert.IsFalse(engine.Enter(Mods.Shift));
        Assert.AreEqual("", engine.Text);
    }

    [TestMethod]
    public void EnterAfterCheckedTextIsNotHeld()
    {
        var (engine, host) = EngineDriver.Create();
        engine.Type(host, "Fine. ");
        engine.Answer(host, "Fine.");

        Assert.IsFalse(engine.Enter());
    }

    [TestMethod]
    public void PauseSendsTextTypedSoFar()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(PauseMs: 1000));
        engine.Type(host, "how ar you");

        engine.Wait(host, 999);
        Assert.AreEqual(0, host.Requests.Count);
        engine.Wait(host, 1);

        Assert.AreEqual("how ar you", host.Requests[0].Text);
        Assert.AreEqual(CorrectionTrigger.Pause, host.Requests[0].Trigger);
    }

    [TestMethod]
    public void RequestNeverStartsInsideAWord()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(PauseMs: 1000));
        engine.Type(host, "ab");
        engine.Wait(host, 1000);
        engine.Answer(host, "ab");

        engine.Type(host, "out tme. ");

        Assert.AreEqual("about tme.", host.Requests[^1].Text);
    }

    [TestMethod]
    public void FailedRequestIsNotRetriedOnEveryPause()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(PauseMs: 1000));
        engine.Type(host, "how ar you");
        engine.Wait(host, 1000);

        engine.OnCorrectionResult(host.Requests[0].Id, CorrectionOutcome.Fail("offline"));
        engine.Wait(host, 5000);

        Assert.AreEqual(1, host.Requests.Count);
    }

    [TestMethod]
    public void BlockedFieldClearsTypedText()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "secret pass. ");

        engine.OnCorrectionResult(host.Requests[0].Id, CorrectionOutcome.Blocked);

        Assert.AreEqual("", engine.Text);
    }

    [TestMethod]
    public void BlockedAppIsNotTracked()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.OnFocusChanged(allowed: false);

        engine.Type(host, "git comit -m. ");

        Assert.AreEqual(0, host.Requests.Count);
        Assert.AreEqual("", engine.Text);
    }

    [TestMethod]
    public void FixWaitsUntilShortcutKeysAreReleased()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");
        host.ShortcutModifiersHeld = true;

        engine.Answer(host, "the cat.");
        Assert.AreEqual(0, host.Injected.Count);

        host.ShortcutModifiersHeld = false;
        engine.Wait(host, TypingEngine.DeferRetryMs);
        Assert.AreEqual("the cat. ", host.Screen);
    }

    [TestMethod]
    public void ShortcutWithoutTypedTextFixesSelection()
    {
        var (engine, host) = EngineDriver.Create();

        engine.OnHotkey();

        Assert.AreEqual(1, host.SelectionFixes);
    }

    [TestMethod]
    public void ShortcutRechecksEverythingTyped()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "One too. ");
        engine.Answer(host, "One too.");

        engine.OnHotkey();

        Assert.AreEqual("One too.", host.Requests[^1].Text);
        Assert.AreEqual(CorrectionTrigger.Shortcut, host.Requests[^1].Trigger);
        Assert.AreEqual(0, host.SelectionFixes);
    }

    [TestMethod]
    public void MouseClickForgetsTypedText()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "abc");

        Assert.IsFalse(engine.OnMouseButton("click", true));
        Assert.AreEqual("", engine.Text);
    }

    [TestMethod]
    public void FailedInjectionForgetsTypedTextAndReplays()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");
        engine.Answer(host, "the cat.");
        engine.Type(host, "x");

        engine.OnInjectionComplete(false);

        Assert.AreEqual("", engine.Text);
        CollectionAssert.AreEqual(new object[] { "key:x" }, host.Replayed);
    }

    [TestMethod]
    public void LostInjectionTimesOut()
    {
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "teh cat. ");
        engine.Answer(host, "the cat.");

        engine.Wait(host, 1000);
        Assert.IsTrue(engine.IsInjecting, "timeout grows with the size of the edit");
        engine.Wait(host, 500);

        Assert.IsFalse(engine.IsInjecting);
    }

    [TestMethod]
    public void PauseNeverCompletesAnUnfinishedWord()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(PauseMs: 1000));
        engine.Type(host, "I am goin");
        engine.Wait(host, 1000);

        engine.Answer(host, "I am going");

        Assert.AreEqual(0, host.Injected.Count);
        Assert.AreEqual("I am goin", host.Screen);
    }

    [TestMethod]
    public void PauseStillFixesOtherWords()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(PauseMs: 1000));
        engine.Type(host, "teh day is goin");
        engine.Wait(host, 1000);

        engine.Answer(host, "the day is going");

        Assert.AreEqual("the day is goin", host.Screen);
    }

    [TestMethod]
    public void FixThatCouldMergeWithTextBeforeTheCaretIsSkipped()
    {
        // The caret was placed after "Hello"; the engine only knows "s there".
        var (engine, host) = EngineDriver.Create(SentenceOnly);
        engine.Type(host, "s there. ");

        engine.Answer(host, "is there.");

        Assert.AreEqual(0, host.Injected.Count);
    }

    [TestMethod]
    public void FocusChangeDuringEnterHoldDropsHeldKeys()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(FixOnPause: false));
        engine.Type(host, "helo wrld");
        engine.Enter();
        engine.Type(host, "x");

        engine.OnFocusChanged(allowed: true);

        Assert.IsFalse(engine.IsHolding);
        Assert.AreEqual(0, host.Replayed.Count);
    }

    [TestMethod]
    public void TimerDoesNotSpinWhileWaitingForAnAnswer()
    {
        var (engine, host) = EngineDriver.Create(new EngineOptions(PauseMs: 1000, FixBeforeEnter: false));
        engine.Type(host, "One. ");
        engine.Type(host, "two");
        engine.Wait(host, 5000);

        Assert.AreEqual(-1, host.TickDelay);
    }
}
