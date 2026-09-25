//! Shrinking policy for read-only context kept only in app memory.

use crate::settings::ContextConfig;

/// Shrink the oldest prefix and report whether any in-memory text was removed.
pub(super) fn shrink(text: &mut String, limits: &ContextConfig) -> bool {
    let Some(cutoff) = shrink_cutoff(text, limits) else {
        return false;
    };
    text.drain(..cutoff);
    true
}

fn shrink_cutoff(text: &str, limits: &ContextConfig) -> Option<usize> {
    let character_count = text.chars().count();
    let excess = character_count.saturating_sub(limits.informative_context_max_chars as usize);
    if excess == 0 {
        return None;
    }
    let budget_cutoff = text
        .char_indices()
        .nth(excess)
        .map_or(text.len(), |(index, _)| index);
    let floor_cutoff = minimum_word_start(text, usize::from(limits.informative_context_min_words));

    // The character budget remains hard. Within it, keep at least the word
    // floor and prefer starting immediately after a configured sentence end.
    let cutoff = floor_cutoff
        .filter(|floor| budget_cutoff <= *floor)
        .and_then(|floor| {
            limits
                .initial_context_boundary_chars
                .iter()
                .filter(|boundary| !boundary.is_empty())
                .flat_map(|boundary| {
                    text.match_indices(boundary)
                        .map(move |(index, _)| index + boundary.len())
                })
                .filter(|candidate| budget_cutoff <= *candidate && *candidate <= floor)
                .min()
        })
        .unwrap_or(budget_cutoff);

    (cutoff > 0).then_some(cutoff)
}

fn minimum_word_start(text: &str, minimum_words: usize) -> Option<usize> {
    if minimum_words == 0 {
        return Some(text.len());
    }
    let mut word_start = 0;
    let mut in_word = false;
    let mut words = 0;
    for (index, character) in text.char_indices().rev() {
        if character.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
                if words == minimum_words {
                    return Some(word_start);
                }
            }
        } else {
            word_start = index;
            in_word = true;
        }
    }
    if in_word {
        words += 1;
    }
    (words >= minimum_words).then_some(word_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_chars: u32, min_words: u16) -> ContextConfig {
        ContextConfig {
            informative_context_max_chars: max_chars,
            informative_context_min_words: min_words,
            ..ContextConfig::default()
        }
    }

    #[test]
    fn prefers_sentence_boundaries() {
        let mut text = "Discard this sentence. Keep these three words".to_owned();

        assert!(shrink(&mut text, &limits(24, 3)));

        assert_eq!(text, " Keep these three words");
    }

    #[test]
    fn preserves_word_floor_when_it_fits() {
        let mut text = "zero one two three four".to_owned();

        assert!(shrink(&mut text, &limits(20, 4)));

        assert_eq!(text.chars().count(), 20);
        assert!(text.split_whitespace().count() >= 4);
    }

    #[test]
    fn keeps_hard_budget_when_word_floor_cannot_fit() {
        let mut text = "alpha beta gamma".to_owned();

        assert!(shrink(&mut text, &limits(10, 3)));

        assert_eq!(text, "beta gamma");
        assert_eq!(text.chars().count(), 10);
    }

    #[test]
    fn is_unicode_safe() {
        let mut text = "old. café 世界".to_owned();

        assert!(shrink(&mut text, &limits(5, 2)));

        assert_eq!(text, "fé 世界");
    }
}
