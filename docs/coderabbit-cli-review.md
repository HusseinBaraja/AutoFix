# CodeRabbit CLI Branch Review

Use CodeRabbit CLI from WSL because the CLI supports Linux and macOS, not native
Windows PowerShell.

## Enter WSL

From PowerShell:

```powershell
wsl
```

Go to the AutoFix repo:

```bash
cd /mnt/c/Users/Hussein/Desktop/Things/Zerone/Projects/AutoFix
```

Or enter WSL directly in the repo:

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

If `cr` is not in `PATH`, use the installed path:

```bash
~/.local/bin/cr --version
```

## Review Local Changes

CodeRabbit's CLI credit email recommends `cr` for feedback on staged and
unstaged changes before committing. Use this as the default review flow:

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

## Useful Review Scopes

Review only uncommitted changes:

```bash
cr review --type uncommitted
```

Review only committed branch changes:

```bash
cr review --base main --type committed
```

Review the current branch against `main`:

```bash
cr review --base main --type all
```

Review one directory:

```bash
cr review --base main --dir crates/app
```

## CLI Credits

The CodeRabbit CLI credit email says the included credits apply to CLI file
reviews. A completed review charges by reviewed file. If `cr stats` shows no
history, the review did not finish and credits were not consumed.

The dashboard may continue to show `$0.00` spent until CodeRabbit records a
completed chargeable CLI review.

Use the same organization shown by:

```bash
cr auth status
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
