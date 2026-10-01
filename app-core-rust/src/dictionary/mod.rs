//! Persistent user exclusions. Session text enters storage only through opt-in learning.
mod learning;
#[cfg(test)]
mod tests;

use crate::correction::{CorrectionChange, CorrectionOutput, LanguageInfo, NoChangeReason};
use crate::settings::{LearningConfig, LearningRule};
pub(crate) use learning::{Learner, Rejection};
use rusqlite::{params, Connection, Result};

#[derive(Default)]
pub(crate) struct Policy {
    pub(crate) terms: Vec<String>,
    pairs: Vec<(String, String)>,
}

pub(crate) struct Repository<'a> {
    connection: &'a Connection,
}

impl<'a> Repository<'a> {
    /// Borrow the engine's exclusion database without creating a separate storage owner.
    pub(crate) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Unknown/mixed language protects all applicable entries conservatively.
    pub(crate) fn policy(&self, app: &str, language: &LanguageInfo) -> Result<Policy> {
        let mut policy = Policy::default();
        let mut statement = self.connection.prepare(
            "select language_code, entry from custom_dictionary_entries
             where app_process_name is null or lower(app_process_name) = lower(?1)",
        )?;
        for row in statement.query_map([app], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (tag, term) = row?;
            if language_matches(Some(&tag), language) {
                policy.terms.push(term);
            }
        }
        let mut statement = self.connection.prepare(
            "select language_code, original_text, rejected_correction from learned_correction_rules
             where learning_enabled = 1 and rule_type = 'pair' and rejected_correction is not null
             and (app_process_name is null or lower(app_process_name) = lower(?1))",
        )?;
        for row in statement.query_map([app], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (tag, original, replacement) = row?;
            if language_matches(tag.as_deref(), language) {
                policy.pairs.push((original, replacement));
            }
        }
        Ok(policy)
    }

    /// Persist an authorized rejection using the selected rule and app scope, deduplicating it.
    /// The learning owner must establish opt-in or consent before calling this method.
    pub(crate) fn remember(&self, rejection: &Rejection, config: &LearningConfig) -> Result<()> {
        let app = config.per_app.then(|| rejection.app.to_ascii_lowercase());
        match config.rule {
            LearningRule::Dictionary => {
                self.connection.execute(
                    "insert into custom_dictionary_entries (language_code, app_process_name, entry)
                     select ?1, ?2, ?3 where not exists (select 1 from custom_dictionary_entries
                     where lower(language_code) = lower(?1) and coalesce(lower(app_process_name), '') = coalesce(?2, '') and entry = ?3)",
                    params![rejection.language.as_deref().unwrap_or("und"), app, rejection.original])?;
            }
            LearningRule::Pair => {
                self.connection.execute(
                    "insert into learned_correction_rules
                     (learning_enabled, original_text, rejected_correction, rule_type, language_code, app_process_name)
                     select 1, ?1, ?2, 'pair', ?3, ?4 where not exists (select 1 from learned_correction_rules
                     where learning_enabled = 1 and rule_type = 'pair' and original_text = ?1
                     and rejected_correction = ?2 and coalesce(lower(language_code), 'und') = coalesce(lower(?3), 'und')
                     and coalesce(lower(app_process_name), '') = coalesce(?4, ''))",
                    params![rejection.original, rejection.corrected, rejection.language, app])?;
            }
        }
        Ok(())
    }
}

/// Match global/base/regional tags, protecting all app rules when detection is uncertain.
fn language_matches(tag: Option<&str>, language: &LanguageInfo) -> bool {
    let Some(tag) = tag else {
        return true;
    };
    if tag.eq_ignore_ascii_case("und") || language.is_uncertain() || language.is_mixed() {
        return true;
    }
    language.primary_language.as_deref().is_none_or(|primary| {
        tag.eq_ignore_ascii_case(primary)
            || (!tag.contains('-')
                && primary
                    .split('-')
                    .next()
                    .is_some_and(|base| tag.eq_ignore_ascii_case(base)))
    })
}

impl Policy {
    /// Remove excluded edits, preserving unrelated changes for either engine.
    pub(crate) fn filter(&self, original: &str, mut output: CorrectionOutput) -> CorrectionOutput {
        if !output.changes_needed {
            return output;
        }
        let Some(changes) = output.changes.as_ref() else {
            // Unstructured results cannot prove that exclusions remain untouched.
            return if self.terms.is_empty() && self.pairs.is_empty() {
                output
            } else {
                CorrectionOutput::unchanged(
                    original.to_owned(),
                    output.confidence,
                    NoChangeReason::AllCandidatesProtected,
                    output.engine_latency_ms,
                )
            };
        };
        let text: Vec<char> = original.chars().collect();
        let mut blocked = vec![false; changes.len()];
        for term in &self.terms {
            for (start, end) in occurrences(&text, term) {
                for (i, change) in changes.iter().enumerate() {
                    blocked[i] |= overlaps(change, start, end);
                }
            }
        }
        for (original, replacement) in &self.pairs {
            for (start, end) in occurrences(&text, original) {
                let inside: Vec<_> = changes
                    .iter()
                    .filter(|c| pair_overlaps(c, start, end))
                    .cloned()
                    .collect();
                if inside.is_empty() {
                    continue;
                }
                if inside
                    .iter()
                    .any(|c| c.start_char < start || c.end_char > end)
                {
                    // Broad edits can bundle the pair with other changes. Allow
                    // a different replacement only when unchanged context can
                    // be trimmed to prove a sole changed word/phrase.
                    for (i, change) in changes
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| pair_overlaps(c, start, end))
                    {
                        let isolated = Rejection::from_undo(
                            &change.original_text,
                            &change.replacement_text,
                            None,
                            String::new(),
                        );
                        let proven_different = isolated.as_ref().is_some_and(|edit| {
                            edit.original.eq_ignore_ascii_case(original)
                                && !edit.corrected.eq_ignore_ascii_case(replacement)
                        });
                        blocked[i] |= !proven_different;
                    }
                    continue;
                }
                if render(&text, start, end, &inside)
                    .is_some_and(|result| result.eq_ignore_ascii_case(replacement))
                {
                    for (i, change) in changes.iter().enumerate() {
                        blocked[i] |= pair_overlaps(change, start, end);
                    }
                }
            }
        }
        if !blocked.iter().any(|b| *b) {
            return output;
        }
        let kept: Vec<_> = changes
            .iter()
            .enumerate()
            .filter(|(i, _)| !blocked[*i])
            .map(|(_, c)| c.clone())
            .collect();
        if kept.is_empty() {
            return CorrectionOutput::unchanged(
                original.to_owned(),
                output.confidence,
                NoChangeReason::AllCandidatesProtected,
                output.engine_latency_ms,
            );
        }
        let Some(corrected) = render(&text, 0, text.len(), &kept) else {
            return CorrectionOutput::unchanged(
                original.to_owned(),
                output.confidence,
                NoChangeReason::AllCandidatesProtected,
                output.engine_latency_ms,
            );
        };
        output.corrected_executable_text = corrected;
        output.changes = Some(kept);
        output
    }
}

/// Detect edits inside a protected term; insertions at its outside boundaries remain allowed.
fn overlaps(change: &CorrectionChange, start: usize, end: usize) -> bool {
    if change.start_char == change.end_char {
        change.start_char > start && change.start_char < end
    } else {
        change.start_char < end && change.end_char > start
    }
}

/// Include boundary insertions when checking the exact outcome of a rejected pair.
fn pair_overlaps(change: &CorrectionChange, start: usize, end: usize) -> bool {
    if change.start_char == change.end_char {
        change.start_char >= start && change.start_char <= end
    } else {
        overlaps(change, start, end)
    }
}

/// Find whole-term matches at Unicode scalar offsets with ASCII-insensitive comparison.
fn occurrences(text: &[char], term: &str) -> Vec<(usize, usize)> {
    let term: Vec<char> = term.chars().collect();
    if term.is_empty() || term.len() > text.len() {
        return Vec::new();
    }
    (0..=text.len() - term.len())
        .filter_map(|start| {
            let end = start + term.len();
            (text[start..end]
                .iter()
                .zip(&term)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
                && (start == 0 || !word_char(text[start - 1]) || !word_char(term[0]))
                && (end == text.len()
                    || !word_char(text[end])
                    || !word_char(*term.last().unwrap())))
            .then_some((start, end))
        })
        .collect()
}

/// Keep letters, digits, underscores and apostrophes within one protected word.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// Render validated, nonoverlapping edits inside a scalar range, refusing mismatched originals.
fn render(text: &[char], start: usize, end: usize, changes: &[CorrectionChange]) -> Option<String> {
    let mut sorted: Vec<_> = changes.iter().collect();
    sorted.sort_by_key(|c| c.start_char);
    let mut cursor = start;
    let mut result = String::new();
    for change in sorted {
        if change.start_char < cursor
            || change.end_char < change.start_char
            || change.end_char > end
        {
            return None;
        }
        if text[change.start_char..change.end_char]
            .iter()
            .collect::<String>()
            != change.original_text
        {
            return None;
        }
        result.extend(&text[cursor..change.start_char]);
        result.push_str(&change.replacement_text);
        cursor = change.end_char;
    }
    result.extend(&text[cursor..end]);
    Some(result)
}
