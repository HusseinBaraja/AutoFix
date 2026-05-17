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
~/.local/bin/coderabbit --version
~/.local/bin/coderabbit auth status
~/.local/bin/coderabbit doctor
```

The short alias should also work:

```bash
~/.local/bin/cr --version
```

## Review the Current Branch

Review the current branch against `main`:

```bash
~/.local/bin/coderabbit review --base main --type all
```

For plain log output:

```bash
~/.local/bin/coderabbit review --base main --type all --no-color
```

For structured agent output:

```bash
~/.local/bin/coderabbit review --base main --type all --agent
```

## Useful Review Scopes

Review only uncommitted changes:

```bash
~/.local/bin/coderabbit review --type uncommitted
```

Review only committed branch changes:

```bash
~/.local/bin/coderabbit review --base main --type committed
```

Review one directory:

```bash
~/.local/bin/coderabbit review --base main --dir crates/app
```

## Long-Running Review

If a review appears stuck, check whether the process is still running:

```bash
ps -eo pid,ppid,stat,comm,args | grep coderabbit
```

Stop a stuck review only when needed:

```bash
pkill -f "coderabbit review"
```
