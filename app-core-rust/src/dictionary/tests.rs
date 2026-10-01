use super::*;
use crate::{
    correction::{ConfidenceTier, CorrectionChangeKind},
    settings::{LearningMode, LearningRule},
    storage::Database,
};

/// Build a known or unknown language snapshot for exclusion scope tests.
fn language(tag: Option<&str>) -> LanguageInfo {
    LanguageInfo {
        primary_language: tag.map(str::to_owned),
        detected_languages: tag.map(|t| vec![t.to_owned()]).unwrap_or_default(),
    }
}
/// Build a typo edit at Unicode scalar offsets.
fn change(start: usize, end: usize, from: &str, to: &str) -> CorrectionChange {
    CorrectionChange {
        start_char: start,
        end_char: end,
        original_text: from.into(),
        replacement_text: to.into(),
        kind: CorrectionChangeKind::Typo,
        explanation: None,
    }
}
/// Build a silent engine result whose categorized edits can be filtered.
fn output(text: &str, edits: Vec<CorrectionChange>) -> CorrectionOutput {
    let mut out = CorrectionOutput::changed(text.into(), ConfidenceTier::High, Some(edits), 1);
    out.behavior = crate::correction::ConfidenceBehavior::Silent;
    out
}

/// App scopes ignore ASCII case, base languages include variants, and unknown text stays protected.
#[test]
fn sqlite_scopes_language_and_apps_and_preserves_unknown_language() {
    let db = Database::open_memory().unwrap();
    let config = LearningConfig {
        mode: LearningMode::Automatic,
        rule: LearningRule::Dictionary,
        per_app: true,
    };
    db.dictionary()
        .remember(
            &Rejection {
                original: "teh phrase".into(),
                corrected: "the phrase".into(),
                language: Some("en".into()),
                app: "Notepad.EXE".into(),
            },
            &config,
        )
        .unwrap();
    assert_eq!(
        db.dictionary()
            .policy("notepad.exe", &language(Some("en-US")))
            .unwrap()
            .terms,
        ["teh phrase"]
    );
    assert!(db
        .dictionary()
        .policy("other.exe", &language(Some("en")))
        .unwrap()
        .terms
        .is_empty());
    assert!(db
        .dictionary()
        .policy("notepad.exe", &language(Some("fr")))
        .unwrap()
        .terms
        .is_empty());
    assert_eq!(
        db.dictionary()
            .policy("notepad.exe", &language(None))
            .unwrap()
            .terms
            .len(),
        1
    );
}

/// Whole-phrase exclusions protect only matching words while unrelated corrections survive.
#[test]
fn protects_whole_phrases_without_matching_inside_words_and_keeps_other_edits() {
    let policy = Policy {
        terms: vec!["teh phrase".into()],
        pairs: vec![],
    };
    let source = "é teh phrase wierd";
    let out = output(
        "é the phrase weird",
        vec![change(2, 5, "teh", "the"), change(13, 18, "wierd", "weird")],
    );
    assert_eq!(
        policy.filter(source, out).corrected_executable_text,
        "é teh phrase weird"
    );
    let policy = Policy {
        terms: vec!["teh".into()],
        pairs: vec![],
    };
    let out = output("thex", vec![change(0, 4, "tehx", "thex")]);
    assert_eq!(policy.filter("tehx", out).corrected_executable_text, "thex");
}

/// Pair rules reject their exact outcome across narrow or broad edits, allowing proven alternatives.
#[test]
fn pair_blocks_only_rejected_replacement_and_supports_multiple_edits_in_phrase() {
    let policy = Policy {
        terms: vec![],
        pairs: vec![("teh".into(), "the".into())],
    };
    let out = output(
        "the weird",
        vec![change(0, 3, "teh", "the"), change(4, 9, "wierd", "weird")],
    );
    assert_eq!(
        policy.filter("teh wierd", out).corrected_executable_text,
        "teh weird"
    );
    let out = output("ten", vec![change(0, 3, "teh", "ten")]);
    assert_eq!(policy.filter("teh", out).corrected_executable_text, "ten");
    assert!(
        !policy
            .filter(
                "I teh today",
                output(
                    "I the today",
                    vec![change(0, 11, "I teh today", "I the today")]
                )
            )
            .changes_needed
    );
    assert_eq!(
        policy
            .filter(
                "I teh today",
                output(
                    "I ten today",
                    vec![change(0, 11, "I teh today", "I ten today")]
                )
            )
            .corrected_executable_text,
        "I ten today"
    );
    assert!(
        !policy
            .filter(
                "teh wierd",
                output("the weird", vec![change(0, 9, "teh wierd", "the weird")])
            )
            .changes_needed
    );
    let policy = Policy {
        terms: vec![],
        pairs: vec![("teh wierd".into(), "the weird".into())],
    };
    let out = output(
        "the weird",
        vec![change(0, 3, "teh", "the"), change(4, 9, "wierd", "weird")],
    );
    assert!(!policy.filter("teh wierd", out).changes_needed);
}

/// Rejected pairs cover boundary insertions and deletions as well as substitutions.
#[test]
fn insertion_and_deletion_rules_are_enforced() {
    let policy = Policy {
        terms: vec![],
        pairs: vec![
            ("cant".into(), "can't".into()),
            ("very very".into(), "very".into()),
        ],
    };
    assert!(
        !policy
            .filter("cant", output("can't", vec![change(3, 3, "", "'")]))
            .changes_needed
    );
    assert!(
        !policy
            .filter("very very", output("very", vec![change(4, 9, " very", "")]))
            .changes_needed
    );
    let policy = Policy {
        terms: vec![],
        pairs: vec![("hello".into(), "hello.".into())],
    };
    assert!(
        !policy
            .filter("hello", output("hello.", vec![change(5, 5, "", ".")]))
            .changes_needed
    );
}

/// Saved pair rules remain active independently of learning and duplicate writes stay unique.
#[test]
fn active_pairs_work_when_learning_is_disabled_and_deduplicate() {
    let db = Database::open_memory().unwrap();
    let rejection = Rejection {
        original: "teh".into(),
        corrected: "the".into(),
        language: Some("en".into()),
        app: "app.exe".into(),
    };
    let config = LearningConfig {
        mode: LearningMode::Automatic,
        ..LearningConfig::default()
    };
    db.dictionary().remember(&rejection, &config).unwrap();
    db.dictionary().remember(&rejection, &config).unwrap();
    let policy = db
        .dictionary()
        .policy("app.exe", &language(Some("en")))
        .unwrap();
    assert_eq!(policy.pairs.len(), 1);
    assert!(
        !policy
            .filter("teh", output("the", vec![change(0, 3, "teh", "the")]))
            .changes_needed
    );
}

/// Engine migrations preserve exclusions in a database first initialized by the settings UI.
#[test]
fn engine_migrates_database_created_by_settings_first() {
    // File-backed migration is exercised through the production Database entry point.
    let path = std::env::temp_dir().join(format!(
        "autofix-dictionary-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = Connection::open(&path).unwrap();
    file.execute_batch("create table custom_dictionary_entries (id integer primary key, language_code text not null, app_process_name text, entry text not null, created_at text not null default current_timestamp, unique(language_code,app_process_name,entry)); insert into custom_dictionary_entries(language_code,entry) values ('en','teh');").unwrap();
    drop(file);
    let db = Database::open(&path).unwrap();
    assert_eq!(
        db.dictionary()
            .policy("app.exe", &language(Some("en")))
            .unwrap()
            .terms,
        ["teh"]
    );
    drop(db);
    std::fs::remove_file(path).unwrap();
}
