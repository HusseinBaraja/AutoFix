//! Conservative admission for terminal/editor prose. Unrecognized syntax refuses work.

use super::{matching_rule, TriggerKind};
use crate::{
    background::{target::FocusedTarget, triggers::CorrectionRequest},
    storage::AppRule,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SafetyMode {
    General,
    Terminal,
    CodeEditor,
    Unknown,
}

pub(super) fn mode(rules: &[AppRule], target: &FocusedTarget) -> SafetyMode {
    // A custom rule can classify an unknown app, but cannot downgrade a known target.
    let process = target.process_name.trim().to_ascii_lowercase();
    let title = target.window_title.to_ascii_lowercase();
    let configured = matching_rule(rules, target)
        .map(|rule| rule.safety_mode.as_str())
        .unwrap_or("auto");
    if process.is_empty()
        || process == "unknown app"
        || !matches!(configured, "auto" | "terminal" | "code_editor")
    {
        return SafetyMode::Unknown;
    }
    if matches!(
        process.as_str(),
        "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "windowsterminal.exe"
            | "wt.exe"
            | "conhost.exe"
            | "bash.exe"
            | "wsl.exe"
            | "mintty.exe"
            | "putty.exe"
            | "wezterm-gui.exe"
            | "alacritty.exe"
            | "hyper.exe"
    ) || configured == "terminal"
        || process.contains("terminal")
        || process.contains("shell")
        || ["terminal", "powershell", "command prompt", "shell", "ssh"]
            .iter()
            .any(|word| title.contains(word))
    {
        return SafetyMode::Terminal;
    }
    if matches!(
        process.as_str(),
        "code.exe"
            | "code-insiders.exe"
            | "devenv.exe"
            | "rider64.exe"
            | "idea64.exe"
            | "pycharm64.exe"
            | "webstorm64.exe"
            | "clion64.exe"
            | "notepad++.exe"
            | "sublime_text.exe"
            | "vim.exe"
            | "nvim.exe"
            | "cursor.exe"
            | "windsurf.exe"
            | "zed.exe"
            | "eclipse.exe"
            | "goland64.exe"
            | "rustrover64.exe"
    ) || [
        "visual studio",
        "intellij",
        "pycharm",
        "webstorm",
        "sublime text",
        "code editor",
    ]
    .iter()
    .any(|word| title.contains(word))
        || title.split_whitespace().any(|part| {
            [
                ".rs", ".py", ".js", ".ts", ".tsx", ".jsx", ".cs", ".cpp", ".c", ".h", ".java",
                ".go", ".rb", ".sql", ".ps1", ".bat", ".cmd", ".sh", ".html", ".css", ".json",
                ".yaml", ".yml", ".toml", ".xml",
            ]
            .iter()
            .any(|extension| part.trim_matches(['\'', '"', '*']).ends_with(extension))
        })
    {
        return SafetyMode::CodeEditor;
    }
    match configured {
        "auto" => SafetyMode::General,
        "terminal" => SafetyMode::Terminal,
        "code_editor" => SafetyMode::CodeEditor,
        _ => SafetyMode::Unknown,
    }
}

pub(super) fn trigger_allowed(
    rules: &[AppRule],
    target: &FocusedTarget,
    trigger: TriggerKind,
) -> bool {
    match mode(rules, target) {
        SafetyMode::General => true,
        SafetyMode::Unknown => false,
        SafetyMode::Terminal | SafetyMode::CodeEditor => {
            let Some(rule) = matching_rule(rules, target) else {
                return false;
            };
            // Manual permission never enables a final automatic fix or continuous capture.
            let allowed = match trigger {
                TriggerKind::Tracking | TriggerKind::FinalFixBeforeReanchor => {
                    rule.word_count_trigger_allowed || rule.character_trigger_allowed
                }
                TriggerKind::ManualShortcut | TriggerKind::Undo => rule.manual_shortcut_allowed,
                TriggerKind::WordCount => rule.word_count_trigger_allowed,
                TriggerKind::Character => rule.character_trigger_allowed,
            };
            allowed
                && (trigger == TriggerKind::Undo
                    || mode(rules, target) != SafetyMode::CodeEditor
                    || rule.prose_context_allowed)
        }
    }
}

pub(in crate::background) fn request_allowed(
    rules: &[AppRule],
    target: &FocusedTarget,
    request: &CorrectionRequest,
) -> bool {
    if !trigger_allowed(rules, target, request.trigger) {
        return false;
    }
    match mode(rules, target) {
        SafetyMode::General => true,
        SafetyMode::Unknown => false,
        SafetyMode::Terminal | SafetyMode::CodeEditor => {
            if mode(rules, target) == SafetyMode::Terminal
                && (request.trigger != TriggerKind::ManualShortcut
                    || request.executable_context.contains(['#', '/'])
                    || request
                        .informative_context
                        .rsplit('\n')
                        .next()
                        .unwrap_or("")
                        .contains(['#', '/'])
                    || request
                        .following_context
                        .split('\n')
                        .next()
                        .unwrap_or("")
                        .contains(['#', '/']))
            {
                // Shell dialect and prompt state are unknown. Neither comment markers
                // nor automatic invocation prove that this is inert prose.
                return false;
            }
            // Native caret/context APIs cannot prove a programming language's grammar.
            // Even explicit trigger permission must pass a deliberate selection and prose test.
            (if request.trigger == TriggerKind::ManualShortcut {
                request.selected_text
            } else {
                request.executable_context.lines().all(|line| {
                    line.trim_start().starts_with('#') || line.trim_start().starts_with("//")
                })
            }) && prose(&request.executable_context)
                && surrounding_line_is_prose(&request.informative_context, true)
                && surrounding_line_is_prose(&request.following_context, false)
        }
    }
}

fn surrounding_line_is_prose(context: &str, preceding: bool) -> bool {
    let line = if preceding {
        context.rsplit('\n').next()
    } else {
        context.split('\n').next()
    }
    .unwrap_or("")
    .trim();
    let line = strip_comment(line);
    line.is_empty() || plain_words(line)
}

fn strip_comment(line: &str) -> &str {
    line.strip_prefix("//")
        .or_else(|| line.strip_prefix('#'))
        .unwrap_or(line)
        .trim()
}

fn prose(text: &str) -> bool {
    !text.trim().is_empty()
        && text.lines().all(|line| {
            let line = strip_comment(line.trim());
            // A single identifier, command or fragment is uncertain. Require a sentence
            // with several words and a common prose word. Unsupported languages skip.
            line.split_whitespace().count() >= 3
                && plain_words(line)
                && line.ends_with(['.', '!', '?'])
                && (line.chars().next().is_some_and(char::is_uppercase)
                    || line.trim_start().starts_with("please "))
                && line.split_whitespace().next().is_some_and(|first| {
                    matches!(
                        first.to_ascii_lowercase().as_str(),
                        "this"
                            | "that"
                            | "these"
                            | "those"
                            | "please"
                            | "the"
                            | "a"
                            | "an"
                            | "i"
                            | "we"
                            | "you"
                            | "it"
                            | "there"
                            | "here"
                            | "our"
                            | "your"
                            | "my"
                            | "their"
                            | "because"
                    )
                })
                && line.split_whitespace().any(|word| {
                    matches!(
                        word.trim_matches(|c: char| !c.is_alphabetic())
                            .to_ascii_lowercase()
                            .as_str(),
                        "the"
                            | "this"
                            | "that"
                            | "these"
                            | "those"
                            | "is"
                            | "are"
                            | "was"
                            | "were"
                            | "please"
                            | "should"
                            | "because"
                            | "with"
                            | "for"
                            | "and"
                            | "we"
                            | "you"
                    )
                })
        })
}

fn plain_words(text: &str) -> bool {
    if !text.chars().all(|c| {
        c.is_alphabetic() || c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | '\'' | '’')
    }) {
        return false;
    }
    let words: Vec<_> = text.split_whitespace().collect();
    let Some(first) = words.first() else {
        return false;
    };
    // Commands can contain only letters and spaces; punctuation alone is not proof.
    if matches!(
        first.to_ascii_lowercase().as_str(),
        "git"
            | "cargo"
            | "dotnet"
            | "npm"
            | "npx"
            | "pip"
            | "python"
            | "node"
            | "ruby"
            | "go"
            | "make"
            | "cmake"
            | "curl"
            | "wget"
            | "ssh"
            | "sudo"
            | "echo"
            | "printf"
            | "cd"
            | "dir"
            | "ls"
            | "rm"
            | "del"
            | "copy"
            | "move"
            | "start"
            | "set"
            | "export"
            | "docker"
            | "kubectl"
            | "winget"
            | "choco"
            | "get"
            | "select"
            | "insert"
            | "update"
            | "delete"
            | "create"
            | "drop"
            | "alter"
            | "return"
            | "import"
            | "from"
            | "use"
            | "let"
            | "const"
            | "var"
            | "fn"
            | "class"
            | "public"
            | "private"
            | "def"
            | "if"
            | "while"
            | "foreach"
    ) {
        return false;
    }
    words.iter().all(|word| {
        // Leading dots can be relative paths; boundary apostrophes can be code literals.
        if word.starts_with(['.', '\'', '’']) {
            return false;
        }
        let word = word.trim_end_matches(['.', ',', '!', '?']);
        if word.ends_with(['\'', '’']) {
            return false;
        }
        if word.is_empty() {
            return false;
        }
        // Embedded dots, punctuation or uppercase transitions identify paths/names.
        if word.contains(['.', ',', '!', '?']) {
            return false;
        }
        let mut chars = word.chars();
        chars.next();
        !chars.any(char::is_uppercase)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::{
        context_capture::SelectionCapture, session::ContextVersions, triggers,
    };
    use crate::{
        background::{
            security::{check_detection, SecurityDecision},
            target::TargetDetection,
        },
        settings::AppConfig,
    };

    fn target(process: &str) -> FocusedTarget {
        FocusedTarget {
            process_id: 42,
            process_name: process.into(),
            window_handle: 123,
            window_title: "Document".into(),
            focused_element_id: None,
            is_elevated: false,
            is_password_or_protected: false,
            is_hidden_or_unavailable: false,
            field_safety_known: true,
            is_secure_desktop: false,
            is_lock_screen: false,
            is_credential_dialog: false,
        }
    }

    fn rule(process: &str) -> AppRule {
        AppRule {
            process_name: process.into(),
            window_title_pattern: None,
            list_behavior: "allowlist".into(),
            manual_shortcut_allowed: true,
            word_count_trigger_allowed: false,
            character_trigger_allowed: false,
            local_engine_allowed: true,
            api_engine_allowed: true,
            safety_mode: "auto".into(),
            prose_context_allowed: false,
        }
    }

    fn request(text: &str) -> CorrectionRequest {
        let mut request = triggers::manual(
            1,
            "",
            text,
            ContextVersions::default(),
            &SelectionCapture::NoSelection,
            &AppConfig::default(),
        )
        .unwrap();
        request.selected_text = true;
        request
    }

    #[test]
    fn deleted_rules_and_manual_only_rules_keep_automatic_paths_disabled() {
        for process in [
            "cmd.exe",
            "pwsh.exe",
            "conhost.exe",
            "bash.exe",
            "code.exe",
            "cursor.exe",
            "devenv.exe",
        ] {
            let target = target(process);
            let mut rule = rule(process);
            rule.prose_context_allowed = true;
            for trigger in [
                TriggerKind::ManualShortcut,
                TriggerKind::Tracking,
                TriggerKind::WordCount,
                TriggerKind::Character,
                TriggerKind::FinalFixBeforeReanchor,
            ] {
                assert!(!trigger_allowed(&[], &target, trigger));
                let rules = [rule.clone()];
                assert_eq!(
                    trigger_allowed(&rules, &target, trigger),
                    trigger == TriggerKind::ManualShortcut
                );
                assert_eq!(
                    matches!(
                        check_detection(
                            trigger,
                            &AppConfig::default(),
                            &rules,
                            TargetDetection::Available(target.clone())
                        ),
                        SecurityDecision::Allowed { .. }
                    ),
                    trigger == TriggerKind::ManualShortcut
                );
            }
        }
    }

    #[test]
    fn editor_requires_prose_opt_in_and_manual_selection_even_with_engine_permission() {
        let target = target("code.exe");
        let mut rule = rule("code.exe");
        let mut request = request("This is teh sentence.");
        assert!(!request_allowed(&[rule.clone()], &target, &request));
        rule.prose_context_allowed = true;
        assert!(request_allowed(&[rule.clone()], &target, &request));
        request.selected_text = false;
        assert!(!request_allowed(&[rule.clone()], &target, &request));
        request.selected_text = true;
        request.informative_context = "let message = \"".into();
        assert!(!request_allowed(&[rule.clone()], &target, &request));
        request.informative_context = "code();\n// ".into();
        assert!(request_allowed(&[rule.clone()], &target, &request));
        request.executable_context = "This is user_name.".into();
        assert!(!request_allowed(&[rule], &target, &request));
    }

    #[test]
    fn automatic_editor_comments_require_both_prose_and_trigger_opt_ins() {
        let target = target("code.exe");
        let mut rule = rule("code.exe");
        let mut request = request("// This is teh comment.");
        request.trigger = TriggerKind::Character;
        request.selected_text = false;
        assert!(!request_allowed(&[rule.clone()], &target, &request));
        rule.character_trigger_allowed = true;
        assert!(!request_allowed(&[rule.clone()], &target, &request));
        rule.prose_context_allowed = true;
        assert!(request_allowed(&[rule.clone()], &target, &request));
        for text in [
            "This is teh sentence.",
            "// This is user_name.",
            "// This is teh comment.\nlet value = teh;",
            "// Fix typo",
            "#define This is teh sentence.",
        ] {
            request.executable_context = text.into();
            assert!(
                !request_allowed(&[rule.clone()], &target, &request),
                "admitted {text}"
            );
        }
    }

    #[test]
    fn terminal_requires_selection_and_rejects_ambiguous_shell_comments() {
        let target = target("cmd.exe");
        let rule = rule("cmd.exe");
        let mut request = request("This is teh sentence.");
        assert!(request_allowed(&[rule.clone()], &target, &request));
        request.selected_text = false;
        assert!(!request_allowed(&[rule.clone()], &target, &request));
        request.selected_text = true;
        for text in [
            "Git commit the changes.",
            "Echo this is teh text.",
            "This is --force please.",
            "This is file.rs.",
            "This is .env.",
            "This is 'teh'.",
            "# This is teh comment.",
            "// This is teh comment.",
            "This is teh sentence",
        ] {
            request.executable_context = text.into();
            assert!(
                !request_allowed(&[rule.clone()], &target, &request),
                "admitted {text}"
            );
        }
    }

    #[test]
    fn custom_profiles_and_title_heuristics_cannot_downgrade_known_apps() {
        let mut target = target("custom.exe");
        let mut rule = rule("custom.exe");
        rule.safety_mode = "code_editor".into();
        assert_eq!(mode(&[rule.clone()], &target), SafetyMode::CodeEditor);
        rule.safety_mode = "terminal".into();
        assert_eq!(mode(&[rule.clone()], &target), SafetyMode::Terminal);
        let mut editor_rule = self::rule("code.exe");
        editor_rule.safety_mode = "terminal".into();
        assert_eq!(
            mode(&[editor_rule], &self::target("code.exe")),
            SafetyMode::Terminal
        );
        rule.safety_mode = "invalid".into();
        assert!(!request_allowed(
            &[rule.clone()],
            &target,
            &request("This is teh text.")
        ));
        target.window_title = "Remote terminal".into();
        rule.safety_mode = "auto".into();
        assert_eq!(mode(&[rule], &target), SafetyMode::Terminal);
        for process in ["CODE.EXE", "cmd.exe"] {
            assert_ne!(
                mode(&[self::rule(process)], &self::target(process)),
                SafetyMode::General
            );
        }
        assert_eq!(mode(&[], &self::target("Unknown app")), SafetyMode::Unknown);
        let mut code_document = self::target("custom.exe");
        code_document.window_title = "script.rs - Text app".into();
        assert_eq!(mode(&[], &code_document), SafetyMode::CodeEditor);
    }

    #[test]
    fn rejects_commands_code_paths_urls_identifiers_and_ambiguous_fragments() {
        for text in [
            "git commit the changes",
            "echo this is teh text",
            "npm run the test",
            "Get-Content the file",
            "Please use --force here.",
            "This is C:\\temp\\teh.txt.",
            "This is /tmp/teh.",
            "This is example.com.",
            "This is https://example.com.",
            "let value = teh;",
            "This is user_name.",
            "This is userName.",
            "This is USER.",
            "This is file.rs.",
            "SELECT the name FROM users",
            "teh",
            "fix typo",
            "Please do $(teh).",
            "This is foo().",
            "This is foo-bar.",
            "This is `teh`.",
            "This is <tag>.",
            "Touch this is teh.",
        ] {
            assert!(!prose(text), "admitted {text}");
        }
        for text in [
            "This is teh sentence.",
            "// This is teh comment.",
            "# This is teh comment.",
            "Please fix teh spelling.",
            "This isn't teh sentence.",
        ] {
            assert!(prose(text), "rejected {text}");
        }
    }

    #[test]
    fn rejects_selections_inside_command_arguments_and_code() {
        assert!(!surrounding_line_is_prose("git commit -m \"", true));
        assert!(!surrounding_line_is_prose("let value = \"", true));
        assert!(!surrounding_line_is_prose("\";\nnext", false));
        assert!(surrounding_line_is_prose("previous code();\n// ", true));
        assert!(surrounding_line_is_prose("\nnext code();", false));
    }
}
