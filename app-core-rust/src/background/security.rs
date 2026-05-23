use crate::{
    settings::{AppConfig, CorrectionEngine, RunMode},
    storage::{AppRule, Database},
};

use super::target::{self, CorrectionEligibility, FocusedTarget, TargetDetection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TriggerKind {
    ManualShortcut,
    WordCount,
    Character,
    FinalFixBeforeReanchor,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SecurityDecision {
    Allowed {
        target: FocusedTarget,
    },
    Blocked {
        reason: BlockReason,
        target: Option<FocusedTarget>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockReason {
    PasswordField,
    ProtectedOrHiddenField,
    SecureDesktop,
    CredentialDialog,
    LockScreen,
    ElevatedTarget,
    SensitiveAppDefault,
    AppRuleBlocked,
    EngineBlocked,
    AllowlistRequired,
    UnsupportedTarget,
}

pub(crate) struct SecurityGate;

impl SecurityGate {
    pub(crate) fn check(
        trigger: TriggerKind,
        config: &AppConfig,
        database: &Database,
    ) -> SecurityDecision {
        let app_rules = match database.app_rules().list() {
            Ok(app_rules) => app_rules,
            Err(error) => {
                tracing::warn!("failed to load app rules for security gate: {}", error);
                Vec::new()
            }
        };

        check_detection(trigger, config, &app_rules, target::detect_focused_target())
    }
}

pub(crate) fn check_detection(
    trigger: TriggerKind,
    config: &AppConfig,
    app_rules: &[AppRule],
    detection: TargetDetection,
) -> SecurityDecision {
    let target = match detection {
        TargetDetection::Available(target) => target,
        TargetDetection::Unsupported => {
            return SecurityDecision::Blocked {
                reason: BlockReason::UnsupportedTarget,
                target: None,
            };
        }
    };

    if let Some(reason) = hard_block_reason(&target) {
        return SecurityDecision::Blocked {
            reason,
            target: Some(target),
        };
    }

    let matching_rule = matching_rule(app_rules, &target);
    if let Some(rule) = matching_rule.filter(|rule| !trigger_allowed(rule, trigger)) {
        let _ = rule;
        return SecurityDecision::Blocked {
            reason: BlockReason::AppRuleBlocked,
            target: Some(target),
        };
    }
    if let Some(rule) =
        matching_rule.filter(|rule| !engine_allowed(rule, &config.correction.engine))
    {
        let _ = rule;
        return SecurityDecision::Blocked {
            reason: BlockReason::EngineBlocked,
            target: Some(target),
        };
    }

    let has_allow_rule = matching_rule.is_some_and(|rule| {
        trigger_allowed(rule, trigger) && engine_allowed(rule, &config.correction.engine)
    });
    if matches!(config.general.run_mode, RunMode::Allowlist) && !has_allow_rule {
        return SecurityDecision::Blocked {
            reason: BlockReason::AllowlistRequired,
            target: Some(target),
        };
    }

    if sensitive_app_default(&target) && !has_allow_rule {
        return SecurityDecision::Blocked {
            reason: BlockReason::SensitiveAppDefault,
            target: Some(target),
        };
    }

    SecurityDecision::Allowed { target }
}

impl TriggerKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ManualShortcut => "manual_shortcut",
            Self::WordCount => "word_count",
            Self::Character => "character",
            Self::FinalFixBeforeReanchor => "final_fix_before_reanchor",
            Self::Undo => "undo",
        }
    }
}

impl BlockReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PasswordField => "password_field",
            Self::ProtectedOrHiddenField => "protected_or_hidden_field",
            Self::SecureDesktop => "secure_desktop",
            Self::CredentialDialog => "credential_dialog",
            Self::LockScreen => "lock_screen",
            Self::ElevatedTarget => "elevated_target",
            Self::SensitiveAppDefault => "sensitive_app_default",
            Self::AppRuleBlocked => "app_rule_blocked",
            Self::EngineBlocked => "engine_blocked",
            Self::AllowlistRequired => "allowlist_required",
            Self::UnsupportedTarget => "unsupported_target",
        }
    }
}

fn engine_allowed(rule: &AppRule, engine: &CorrectionEngine) -> bool {
    if rule.list_behavior == "blocklist" {
        return false;
    }

    match engine {
        CorrectionEngine::Local => rule.local_engine_allowed,
        CorrectionEngine::Api => rule.api_engine_allowed,
    }
}

fn hard_block_reason(target: &FocusedTarget) -> Option<BlockReason> {
    match target.correction_eligibility() {
        CorrectionEligibility::Allowed => None,
        CorrectionEligibility::BlockedSecureDesktop => Some(BlockReason::SecureDesktop),
        CorrectionEligibility::BlockedLockScreen => Some(BlockReason::LockScreen),
        CorrectionEligibility::BlockedCredentialDialog => Some(BlockReason::CredentialDialog),
        CorrectionEligibility::BlockedElevated => Some(BlockReason::ElevatedTarget),
        CorrectionEligibility::BlockedProtectedField => Some(BlockReason::PasswordField),
        CorrectionEligibility::BlockedHiddenOrUnavailable => {
            Some(BlockReason::ProtectedOrHiddenField)
        }
        CorrectionEligibility::Unsupported => Some(BlockReason::UnsupportedTarget),
    }
}

fn matching_rule<'a>(app_rules: &'a [AppRule], target: &FocusedTarget) -> Option<&'a AppRule> {
    let process_name = normalize(&target.process_name);
    app_rules.iter().find(|rule| {
        normalize(&rule.process_name) == process_name
            && rule
                .window_title_pattern
                .as_ref()
                .is_none_or(|pattern| title_matches(&target.window_title, pattern))
    })
}

fn trigger_allowed(rule: &AppRule, trigger: TriggerKind) -> bool {
    if rule.list_behavior == "blocklist" {
        return false;
    }

    match trigger {
        TriggerKind::ManualShortcut => rule.manual_shortcut_allowed,
        TriggerKind::WordCount => rule.word_count_trigger_allowed,
        TriggerKind::Character => rule.character_trigger_allowed,
        TriggerKind::FinalFixBeforeReanchor => {
            rule.manual_shortcut_allowed
                || rule.word_count_trigger_allowed
                || rule.character_trigger_allowed
        }
        TriggerKind::Undo => rule.manual_shortcut_allowed,
    }
}

fn sensitive_app_default(target: &FocusedTarget) -> bool {
    let process_name = normalize(&target.process_name);
    let window_title = normalize(&target.window_title);
    is_curated_sensitive_process(&process_name)
        || contains_sensitive_keyword(&process_name)
        || contains_sensitive_keyword(&window_title)
}

fn is_curated_sensitive_process(process_name: &str) -> bool {
    matches!(
        process_name,
        "1password.exe"
            | "bitwarden.exe"
            | "dashlane.exe"
            | "enpass.exe"
            | "keepass.exe"
            | "keepassxc.exe"
            | "lastpass.exe"
            | "nordpass.exe"
            | "proton pass.exe"
            | "roboform.exe"
            | "keeper.exe"
            | "credentialui.exe"
            | "credwiz.exe"
            | "consent.exe"
            | "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "windowsterminal.exe"
            | "wt.exe"
            | "regedit.exe"
            | "regedt32.exe"
            | "taskmgr.exe"
            | "services.exe"
            | "mmc.exe"
            | "compmgmt.msc"
            | "secpol.msc"
            | "gpedit.msc"
            | "mstsc.exe"
            | "teamviewer.exe"
            | "anydesk.exe"
            | "putty.exe"
            | "winscp.exe"
            | "openvpn-gui.exe"
            | "wireguard.exe"
    )
}

fn contains_sensitive_keyword(value: &str) -> bool {
    [
        "password",
        "passkey",
        "credential",
        "secret",
        "vault",
        "wallet",
        "bank",
        "banking",
        "finance",
        "trading",
        "brokerage",
        "admin",
        "administrator",
        "elevated",
        "registry",
        "terminal",
        "shell",
        "remote desktop",
        "rdp",
        "ssh",
        "vpn",
    ]
    .iter()
    .any(|keyword| value.contains(keyword))
}

fn title_matches(title: &str, pattern: &str) -> bool {
    let title = normalize(title);
    let pattern = normalize(pattern);
    if pattern == "*" {
        return true;
    }

    if pattern.contains('*') {
        let parts = pattern
            .split('*')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        return parts.iter().all(|part| title.contains(part));
    }

    title.contains(&pattern)
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(crate) fn looks_code_like_or_command_like(text: &str) -> bool {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.is_empty() {
        return false;
    }

    if lower.contains("://")
        || lower.starts_with("www.")
        || lower.contains("\\")
        || lower.contains("~/")
        || lower.contains("./")
        || lower.contains("../")
    {
        return true;
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.iter().any(|token| {
        token.starts_with("--")
            || (token.starts_with('-') && token.len() > 1)
            || token.starts_with('/')
            || token.contains("::")
            || token.contains("=>")
            || token.contains("->")
    }) {
        return true;
    }

    if [
        "git ",
        "cargo ",
        "dotnet ",
        "npm ",
        "pnpm ",
        "yarn ",
        "python ",
        "pip ",
        "ssh ",
        "cd ",
        "dir ",
        "ls ",
        "mkdir ",
        "rm ",
        "del ",
        "copy ",
        "xcopy ",
        "robocopy ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }

    let code_markers = [
        "{", "}", ";", "==", "!=", "<=", ">=", "&&", "||", "fn ", "let ", "var ", "const ",
        "public ", "private ", "class ", "using ",
    ];
    code_markers.iter().any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::RunMode;

    fn target(process_name: &str) -> FocusedTarget {
        FocusedTarget {
            process_id: 42,
            process_name: process_name.to_owned(),
            window_handle: 123,
            window_title: "Notes".to_owned(),
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

    fn allow_rule(process_name: &str) -> AppRule {
        AppRule {
            process_name: process_name.to_owned(),
            window_title_pattern: None,
            list_behavior: "allowlist".to_owned(),
            manual_shortcut_allowed: true,
            word_count_trigger_allowed: true,
            character_trigger_allowed: true,
            local_engine_allowed: true,
            api_engine_allowed: true,
        }
    }

    fn block_rule(process_name: &str) -> AppRule {
        AppRule {
            list_behavior: "blocklist".to_owned(),
            manual_shortcut_allowed: false,
            word_count_trigger_allowed: false,
            character_trigger_allowed: false,
            local_engine_allowed: false,
            api_engine_allowed: false,
            ..allow_rule(process_name)
        }
    }

    fn terminal_default_rule(process_name: &str) -> AppRule {
        AppRule {
            process_name: process_name.to_owned(),
            window_title_pattern: None,
            list_behavior: "allowlist".to_owned(),
            manual_shortcut_allowed: false,
            word_count_trigger_allowed: false,
            character_trigger_allowed: false,
            local_engine_allowed: true,
            api_engine_allowed: true,
        }
    }

    fn check(config: &AppConfig, rules: &[AppRule], target: FocusedTarget) -> SecurityDecision {
        check_detection(
            TriggerKind::ManualShortcut,
            config,
            rules,
            TargetDetection::Available(target),
        )
    }

    #[test]
    fn password_field_blocks_even_with_allow_rule() {
        let mut target = target("notepad.exe");
        target.is_password_or_protected = true;

        assert!(matches!(
            check(&AppConfig::default(), &[allow_rule("notepad.exe")], target),
            SecurityDecision::Blocked {
                reason: BlockReason::PasswordField,
                ..
            }
        ));
    }

    #[test]
    fn protected_or_hidden_field_blocks_even_with_allow_rule() {
        let mut target = target("notepad.exe");
        target.is_hidden_or_unavailable = true;

        assert!(matches!(
            check(&AppConfig::default(), &[allow_rule("notepad.exe")], target),
            SecurityDecision::Blocked {
                reason: BlockReason::ProtectedOrHiddenField,
                ..
            }
        ));
    }

    #[test]
    fn secure_desktop_lock_screen_and_credential_dialog_are_hard_blocks() {
        let mut secure = target("notepad.exe");
        secure.is_secure_desktop = true;
        let mut locked = target("notepad.exe");
        locked.is_lock_screen = true;
        let mut credential = target("notepad.exe");
        credential.is_credential_dialog = true;
        let rules = [allow_rule("notepad.exe")];

        assert!(matches!(
            check(&AppConfig::default(), &rules, secure),
            SecurityDecision::Blocked {
                reason: BlockReason::SecureDesktop,
                ..
            }
        ));
        assert!(matches!(
            check(&AppConfig::default(), &rules, locked),
            SecurityDecision::Blocked {
                reason: BlockReason::LockScreen,
                ..
            }
        ));
        assert!(matches!(
            check(&AppConfig::default(), &rules, credential),
            SecurityDecision::Blocked {
                reason: BlockReason::CredentialDialog,
                ..
            }
        ));
    }

    #[test]
    fn sensitive_app_blocks_by_default() {
        assert!(matches!(
            check(&AppConfig::default(), &[], target("Bitwarden.exe")),
            SecurityDecision::Blocked {
                reason: BlockReason::SensitiveAppDefault,
                ..
            }
        ));
    }

    #[test]
    fn sensitive_app_allow_rule_permits_when_field_is_clearly_safe() {
        assert!(matches!(
            check(
                &AppConfig::default(),
                &[allow_rule("Bitwarden.exe")],
                target("Bitwarden.exe")
            ),
            SecurityDecision::Allowed { .. }
        ));
    }

    #[test]
    fn sensitive_app_allow_rule_cannot_override_unknown_field_safety() {
        let mut target = target("Bitwarden.exe");
        target.field_safety_known = false;

        assert!(matches!(
            check(
                &AppConfig::default(),
                &[allow_rule("Bitwarden.exe")],
                target
            ),
            SecurityDecision::Blocked {
                reason: BlockReason::UnsupportedTarget,
                ..
            }
        ));
    }

    #[test]
    fn block_rule_blocks_matching_app() {
        assert!(matches!(
            check(
                &AppConfig::default(),
                &[block_rule("notepad.exe")],
                target("notepad.exe")
            ),
            SecurityDecision::Blocked {
                reason: BlockReason::AppRuleBlocked,
                ..
            }
        ));
    }

    #[test]
    fn allowlist_mode_blocks_without_allow_rule() {
        let mut config = AppConfig::default();
        config.general.run_mode = RunMode::Allowlist;

        assert!(matches!(
            check(&config, &[], target("notepad.exe")),
            SecurityDecision::Blocked {
                reason: BlockReason::AllowlistRequired,
                ..
            }
        ));
    }

    #[test]
    fn allowlist_mode_allows_with_allow_rule() {
        let mut config = AppConfig::default();
        config.general.run_mode = RunMode::Allowlist;

        assert!(matches!(
            check(&config, &[allow_rule("notepad.exe")], target("notepad.exe")),
            SecurityDecision::Allowed { .. }
        ));
    }

    #[test]
    fn engine_blocked_when_matching_rule_disallows_selected_engine() {
        let mut config = AppConfig::default();
        config.correction.engine = CorrectionEngine::Api;
        let mut rule = allow_rule("notepad.exe");
        rule.api_engine_allowed = false;

        assert!(matches!(
            check(&config, &[rule], target("notepad.exe")),
            SecurityDecision::Blocked {
                reason: BlockReason::EngineBlocked,
                ..
            }
        ));
    }

    #[test]
    fn allowlist_mode_requires_engine_allowed() {
        let mut config = AppConfig::default();
        config.general.run_mode = RunMode::Allowlist;
        config.correction.engine = CorrectionEngine::Api;
        let mut rule = allow_rule("notepad.exe");
        rule.api_engine_allowed = false;

        assert!(matches!(
            check(&config, &[rule], target("notepad.exe")),
            SecurityDecision::Blocked {
                reason: BlockReason::EngineBlocked,
                ..
            }
        ));
    }

    #[test]
    fn terminal_default_rule_blocks_manual_and_automatic_triggers() {
        let rule = terminal_default_rule("cmd.exe");
        for trigger in [
            TriggerKind::ManualShortcut,
            TriggerKind::WordCount,
            TriggerKind::Character,
        ] {
            assert!(matches!(
                check_detection(
                    trigger,
                    &AppConfig::default(),
                    std::slice::from_ref(&rule),
                    TargetDetection::Available(target("cmd.exe"))
                ),
                SecurityDecision::Blocked {
                    reason: BlockReason::AppRuleBlocked,
                    ..
                }
            ));
        }
    }

    #[test]
    fn explicit_terminal_rule_can_enable_manual_shortcut() {
        let mut rule = terminal_default_rule("cmd.exe");
        rule.manual_shortcut_allowed = true;

        assert!(matches!(
            check_detection(
                TriggerKind::ManualShortcut,
                &AppConfig::default(),
                &[rule],
                TargetDetection::Available(target("cmd.exe"))
            ),
            SecurityDecision::Allowed { .. }
        ));
    }

    #[test]
    fn every_trigger_uses_same_policy_entrypoint() {
        for trigger in [
            TriggerKind::ManualShortcut,
            TriggerKind::WordCount,
            TriggerKind::Character,
            TriggerKind::FinalFixBeforeReanchor,
            TriggerKind::Undo,
        ] {
            assert!(matches!(
                check_detection(
                    trigger,
                    &AppConfig::default(),
                    &[],
                    TargetDetection::Available(target("Bitwarden.exe"))
                ),
                SecurityDecision::Blocked {
                    reason: BlockReason::SensitiveAppDefault,
                    ..
                }
            ));
        }
    }

    #[test]
    fn unsupported_target_blocks_without_target_context() {
        assert_eq!(
            check_detection(
                TriggerKind::ManualShortcut,
                &AppConfig::default(),
                &[],
                TargetDetection::Unsupported
            ),
            SecurityDecision::Blocked {
                reason: BlockReason::UnsupportedTarget,
                target: None,
            }
        );
    }

    #[test]
    fn window_title_pattern_matches_case_insensitive_substrings() {
        let mut target = target("word.exe");
        target.window_title = "Quarterly Admin Notes".to_owned();
        let mut rule = allow_rule("word.exe");
        rule.window_title_pattern = Some("admin".to_owned());

        assert!(matching_rule(&[rule], &target).is_some());
    }

    #[test]
    fn code_like_heuristic_detects_commands_paths_urls_and_syntax() {
        assert!(looks_code_like_or_command_like("git commit -m fix"));
        assert!(looks_code_like_or_command_like(r"C:\Users\me\file.txt"));
        assert!(looks_code_like_or_command_like("https://example.test/path"));
        assert!(looks_code_like_or_command_like("let value = foo::bar();"));
        assert!(!looks_code_like_or_command_like(
            "Please correct this sentence."
        ));
    }
}
