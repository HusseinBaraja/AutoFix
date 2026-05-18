# CodeRabbit CLI Branch Review

Use CodeRabbit CLI from WSL.

## Enter WSL

From PowerShell:

```powershell
wsl --cd /mnt/c/Users/Hussein/Desktop/Things/Zerone/Projects/AutoFix
```

Exit WSL:

```bash
exit
```

## Verify CodeRabbit

```bash
cr --version
cr auth status
cr doctor
```

## Review Local Changes

```bash
cr
```

This is equivalent to reviewing local changes:

```bash
cr review
```

For structured output that an AI agent can act on:

```bash
cr --agent
```

For plain log output:

```bash
cr --plain --no-color
```

Check local review history and usage stats:

```bash
cr stats
```

## Output Options

CodeRabbit CLI does not currently have an official Markdown output mode.

Official modes:

```bash
cr                # default plain text review
cr --plain        # detailed plain text review
cr --interactive  # terminal UI
cr --agent        # structured JSON stream for agents
```

Save plain text into a Markdown file:

```bash
cr --plain --no-color > coderabbit-review.md
```

This creates a `.md` file, but the content is still CodeRabbit plain text.

Save agent output for later conversion:

```bash
cr --agent > coderabbit-review.jsonl
```

Use `coderabbit-review.jsonl` when an AI agent should turn findings into a
Markdown report or act on the structured findings.

## Useful Review Scopes

Review the current branch against `main`:

```bash
cr review --base main --type all
```



## Long-Running Review

If a review appears stuck, check whether the process is still running:

```bash
ps -eo pid,ppid,stat,comm,args | grep coderabbit
```

Stop a stuck review only when needed:

```bash
pkill -f "coderabbit review"
pkill -f "coderabbit --plain"
pkill -f "coderabbit --agent"
```
