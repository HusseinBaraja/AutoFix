# AutoFix

AI autocorrect for every app on Windows. Paste an API key, then type normally:
AutoFix fixes your typos in place, in any text box, as fast as your AI provider answers.

```
You type:     i hav a problm wiht teh code. |
A moment later: i have a problem with the code. |     ✓ hav → have · problm wiht teh → problem with the
```

## Get started

1. Run `AutoFix.exe`. The settings window opens.
2. Choose a provider (DeepSeek is the default), paste your API key, and click **Save and test**.
   Pasting a key tests it right away. Keys that start with `sk-ant-`, `gsk_`, `AIza` or `sk-or-`
   select their provider automatically.
3. Type in the **Try it** box, or in any app. AutoFix runs in the tray.

Supported providers: DeepSeek, OpenAI, Anthropic Claude, Google Gemini, Groq, OpenRouter,
Mistral, Ollama (local, no key), and any other OpenAI-compatible server.

## When AutoFix fixes text

| When | What happens |
| --- | --- |
| You finish a sentence (`.` `!` `?` then a space) | That sentence is fixed while you keep typing. |
| You pause typing (default 1 s) | Text typed so far is fixed. Good for chat. |
| You press Enter | Enter waits for the fix (at most 3 s), so messages are sent corrected. Shift+Enter is never held. |
| You press **Ctrl+Alt+Space** | Fixes the selected text, or what you just typed. |

**Undo:** press Backspace right after a fix. The original text comes back, and its words go to
**Words to keep** so AutoFix leaves them alone next time.

Modes: **Spelling only** (default; keeps your grammar, slang, capitalization and punctuation)
or **Spelling and grammar**.

## How it is fast

- AutoFix watches the keys you type, so it knows the text before the caret without reading the app.
- A fix is typed directly: AutoFix deletes back only to the first changed character and retypes the rest.
  No clipboard, no polling, and no extra round trip.
- Keys you press while a fix is typed are held for a few milliseconds and replayed in order, so nothing is lost.
- The HTTPS connection to the provider stays open, and the password-field check starts when you
  begin typing. A fix costs one API round trip plus about 1 ms per typed key event.

## Privacy and safety

- Only the text you just typed (and up to 400 earlier characters as context) is sent, and only when a fix runs.
- Password fields are never read. Single tokens such as emails, links, and codes are never sent automatically.
- Terminals, code editors, password managers and remote desktops are off by default. Turn AutoFix
  off for any app from the tray menu or in settings.
- AutoFix never changes text after the caret. If you click, move the caret, or switch windows, it
  forgets what you typed, and late answers are dropped.
- Answers that change too much (for example, the model answered a question instead of fixing it) are ignored.
- API keys are stored in Windows Credential Manager. Settings are in
  `%LOCALAPPDATA%\AutoFix\settings.json`. The log (`autofix.log`, same folder) holds timings and errors, never text.

## Build from source

Requires the .NET 8 SDK on Windows 10 or 11.

```powershell
.\scripts\run.ps1                 # build and start
dotnet test .\tests\AutoFix.Tests # unit tests
.\scripts\test-e2e.ps1            # end-to-end: real app, mock AI server, real key events (takes focus ~20 s)
.\scripts\publish.ps1             # dist\AutoFix.exe, one self-contained file
```

## Project layout

- `src/AutoFix.Core` — platform-free logic: the typing engine (state machine), edit planning, prompt,
  provider clients, response checks, settings. Fully unit tested.
- `src/AutoFix` — the Windows app: keyboard and mouse hooks on a dedicated input thread, `SendInput`
  edits, password and caret detection, clipboard-safe selection fixes, tray, settings window.
- `tests/AutoFix.Tests` — unit tests. `tests/AutoFix.E2E` — end-to-end harness.

## Limits

- Text entered with an IME (Chinese, Japanese, Korean) is not tracked.
- Apps that run as administrator cannot be corrected unless AutoFix also runs as administrator.
- Editors that change text by themselves while you type (for example, their own autocorrect) can
  make AutoFix skip a fix.
- After a pause, AutoFix never completes the last word, because you may still be typing it.
- When you start typing in the middle of existing text, a fix that would change the first letter of
  the first word is skipped, because AutoFix cannot see the letters before the caret.
- Selection fixes use the clipboard briefly; the app you copy from may still add the selection to
  Windows clipboard history.
