use std::time::Instant;

use super::{
    ConfidenceBehavior, ConfidenceTier, CorrectionChange, CorrectionChangeKind, CorrectionInput,
    CorrectionMode, CorrectionOutput, GrammarCategory, NoChangeReason,
};

#[derive(Debug)]
struct Word<'a> {
    text: &'a str,
    start_byte: usize,
    end_byte: usize,
    start_char: usize,
    end_char: usize,
}

#[derive(Debug)]
struct Candidate {
    start_byte: usize,
    end_byte: usize,
    start_char: usize,
    end_char: usize,
    replacement: String,
    kind: CorrectionChangeKind,
    confidence: ConfidenceTier,
}

/// Applies conservative English rules only to the executable span.
pub(super) fn correct(input: &CorrectionInput) -> CorrectionOutput {
    let started = Instant::now();
    let original = &input.executable_context;

    if !supports_english(input) {
        return CorrectionOutput::unchanged(
            original.clone(),
            ConfidenceTier::Low,
            NoChangeReason::UnsupportedLanguage,
            elapsed_ms(started),
        );
    }

    let words = words(original);
    let protected = protected_ranges(input, &words);
    let mut candidates = typo_candidates(&words);

    if input.mode == CorrectionMode::TyposPlusGrammar {
        grammar_candidates(input, &words, &mut candidates);
    }

    // Prefer typo edits, then article replacements that can include enabled
    // capitalization, when conservative rules share a source span.
    candidates.sort_by_key(|candidate| {
        (
            candidate.start_byte,
            candidate.end_byte,
            match candidate.kind {
                CorrectionChangeKind::Typo => 0,
                CorrectionChangeKind::Grammar(GrammarCategory::Articles) => 1,
                CorrectionChangeKind::Grammar(_) => 2,
            },
        )
    });

    let mut selected: Vec<Candidate> = Vec::new();
    let mut protected_count = 0;
    let mut suppressed_count = 0;
    let mut blocked_confidence = None;

    for candidate in candidates {
        if intersects_any(candidate.start_byte, candidate.end_byte, &protected) {
            protected_count += 1;
            lower_confidence(&mut blocked_confidence, candidate.confidence);
            continue;
        }
        if behavior_for(input, candidate.confidence) == ConfidenceBehavior::DoNothing {
            suppressed_count += 1;
            lower_confidence(&mut blocked_confidence, candidate.confidence);
            continue;
        }
        if selected.iter().any(|existing| {
            ranges_intersect(
                candidate.start_byte,
                candidate.end_byte,
                existing.start_byte,
                existing.end_byte,
            )
        }) {
            continue;
        }
        selected.push(candidate);
    }

    if selected.is_empty() {
        let reason = if suppressed_count > 0 {
            NoChangeReason::ConfidenceBelowConfiguredBehavior
        } else if protected_count > 0 {
            NoChangeReason::AllCandidatesProtected
        } else {
            NoChangeReason::NoCorrectionNeeded
        };
        return CorrectionOutput::unchanged(
            original.clone(),
            blocked_confidence.unwrap_or(ConfidenceTier::High),
            reason,
            elapsed_ms(started),
        );
    }

    selected.sort_by_key(|candidate| candidate.start_byte);
    let confidence = selected
        .iter()
        .map(|candidate| candidate.confidence)
        .min_by_key(|tier| confidence_rank(*tier))
        .unwrap_or(ConfidenceTier::Low);

    let changes = selected
        .iter()
        .map(|candidate| CorrectionChange {
            start_char: candidate.start_char,
            end_char: candidate.end_char,
            original_text: original[candidate.start_byte..candidate.end_byte].to_owned(),
            replacement_text: candidate.replacement.clone(),
            kind: candidate.kind.clone(),
            explanation: None,
        })
        .collect();

    let mut corrected = original.clone();
    for candidate in selected.iter().rev() {
        corrected.replace_range(
            candidate.start_byte..candidate.end_byte,
            &candidate.replacement,
        );
    }

    CorrectionOutput::changed(corrected, confidence, Some(changes), elapsed_ms(started))
}

/// Converts elapsed time to a saturating millisecond count.
fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

/// Allows English when the preferred or detected language supports it.
fn supports_english(input: &CorrectionInput) -> bool {
    if let Some(primary) = &input.language_info.primary_language {
        return is_english_tag(primary);
    }

    input.language_info.detected_languages.is_empty()
        || input
            .language_info
            .detected_languages
            .iter()
            .any(|language| is_english_tag(language))
}

/// Recognizes English BCP 47 tags without treating other languages as English.
fn is_english_tag(language: &str) -> bool {
    language.eq_ignore_ascii_case("en")
        || language
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("en-"))
}

/// Selects the configured action for a candidate's confidence tier.
fn behavior_for(input: &CorrectionInput, tier: ConfidenceTier) -> ConfidenceBehavior {
    match tier {
        ConfidenceTier::High => input.confidence_behavior.high,
        ConfidenceTier::Medium => input.confidence_behavior.medium,
        ConfidenceTier::Low => input.confidence_behavior.low,
    }
}

/// Ranks confidence from low to high for conservative result reporting.
const fn confidence_rank(tier: ConfidenceTier) -> u8 {
    match tier {
        ConfidenceTier::Low => 0,
        ConfidenceTier::Medium => 1,
        ConfidenceTier::High => 2,
    }
}

/// Retains the lowest confidence among blocked candidates.
fn lower_confidence(current: &mut Option<ConfidenceTier>, candidate: ConfidenceTier) {
    if current.is_none_or(|tier| confidence_rank(candidate) < confidence_rank(tier)) {
        *current = Some(candidate);
    }
}

/// Finds known misspellings while preserving case and sentence starts.
fn typo_candidates(words: &[Word<'_>]) -> Vec<Candidate> {
    words
        .iter()
        .filter_map(|word| {
            let lower = word.text.to_ascii_lowercase();
            let (replacement, confidence) = typo_replacement(&lower)?;
            let replacement = preserve_case(word.text, replacement)?;
            Some(word_candidate(
                word,
                replacement,
                CorrectionChangeKind::Typo,
                confidence,
            ))
        })
        .collect()
}

/// Looks up a conservative typo replacement and its confidence.
fn typo_replacement(word: &str) -> Option<(&'static str, ConfidenceTier)> {
    use ConfidenceTier::{High, Medium};

    Some(match word {
        "teh" | "hte" => ("the", High),
        "adn" => ("and", High),
        "taht" => ("that", High),
        "becuase" => ("because", High),
        "recieve" => ("receive", High),
        "seperate" => ("separate", High),
        "definately" => ("definitely", High),
        "tommorow" => ("tomorrow", High),
        "untill" => ("until", High),
        "wierd" => ("weird", High),
        "mispelling" => ("misspelling", High),
        "grammer" => ("grammar", High),
        "writting" => ("writing", High),
        "comming" => ("coming", High),
        "arguement" => ("argument", High),
        "begining" => ("beginning", High),
        "calender" => ("calendar", High),
        "enviroment" => ("environment", High),
        "goverment" => ("government", High),
        "happend" => ("happened", High),
        "occured" => ("occurred", High),
        "succesful" => ("successful", High),
        "treshold" => ("threshold", High),
        "usualy" => ("usually", High),
        "usefull" => ("useful", High),
        "similiar" => ("similar", High),
        "seperately" => ("separately", High),
        "relevent" => ("relevant", High),
        "refered" => ("referred", High),
        "priviledge" => ("privilege", High),
        "independant" => ("independent", High),
        "existance" => ("existence", High),
        "embarass" => ("embarrass", High),
        "concious" => ("conscious", High),
        "occassion" => ("occasion", High),
        "publically" => ("publicly", Medium),
        "rythm" => ("rhythm", Medium),
        "accomodate" => ("accommodate", Medium),
        "adress" => ("address", Medium),
        "alot" => ("a lot", Medium),
        _ => return None,
    })
}

/// Accepts only a known spelling replacement when an API labels an edit a typo.
pub(super) fn is_known_typo_change(original: &str, replacement: &str) -> bool {
    typo_replacement(&original.to_ascii_lowercase())
        .and_then(|(expected, _)| preserve_case(original, expected))
        .is_some_and(|expected| expected == replacement)
}

/// Adds candidates only for enabled grammar categories.
fn grammar_candidates(input: &CorrectionInput, words: &[Word<'_>], out: &mut Vec<Candidate>) {
    if grammar_enabled(input, GrammarCategory::Capitalization) {
        for word in words {
            if word.text == "i" {
                out.push(word_candidate(
                    word,
                    "I".to_owned(),
                    CorrectionChangeKind::Grammar(GrammarCategory::Capitalization),
                    ConfidenceTier::High,
                ));
            } else if word.text.chars().next().is_some_and(char::is_lowercase)
                && is_sentence_start(input, word.start_byte)
            {
                out.push(word_candidate(
                    word,
                    capitalize_first(word.text),
                    CorrectionChangeKind::Grammar(GrammarCategory::Capitalization),
                    ConfidenceTier::Medium,
                ));
            }
        }
    }

    if grammar_enabled(input, GrammarCategory::Agreement) {
        for pair in words.windows(2) {
            if !only_whitespace_between(input, &pair[0], &pair[1]) {
                continue;
            }
            let subject = pair[0].text.to_ascii_lowercase();
            let verb = pair[1].text.to_ascii_lowercase();
            if let Some(replacement) = agreement_replacement(&subject, &verb) {
                if let Some(replacement) = preserve_case(pair[1].text, replacement) {
                    out.push(word_candidate(
                        &pair[1],
                        replacement,
                        CorrectionChangeKind::Grammar(GrammarCategory::Agreement),
                        ConfidenceTier::High,
                    ));
                }
            }
        }
    }

    if grammar_enabled(input, GrammarCategory::Tense) {
        for pair in words.windows(2) {
            if !only_whitespace_between(input, &pair[0], &pair[1]) {
                continue;
            }
            let auxiliary = pair[0].text.to_ascii_lowercase();
            let verb = pair[1].text.to_ascii_lowercase();
            if let Some(replacement) = tense_replacement(&auxiliary, &verb) {
                if let Some(replacement) = preserve_case(pair[1].text, replacement) {
                    out.push(word_candidate(
                        &pair[1],
                        replacement,
                        CorrectionChangeKind::Grammar(GrammarCategory::Tense),
                        ConfidenceTier::High,
                    ));
                }
            }
        }
    }

    if grammar_enabled(input, GrammarCategory::Spacing) {
        spacing_candidates(&input.executable_context, out);
    }
    if grammar_enabled(input, GrammarCategory::ExtraPunctuation) {
        extra_punctuation_candidates(&input.executable_context, out);
    }
    if grammar_enabled(input, GrammarCategory::MissingPunctuation)
        && input.trigger_type == super::TriggerType::ManualShortcut
        && words.len() >= 3
        && input
            .executable_context
            .chars()
            .last()
            .is_some_and(char::is_alphabetic)
    {
        let at = input.executable_context.len();
        let char_at = input.executable_context.chars().count();
        out.push(Candidate {
            start_byte: at,
            end_byte: at,
            start_char: char_at,
            end_char: char_at,
            replacement: ".".into(),
            kind: CorrectionChangeKind::Grammar(GrammarCategory::MissingPunctuation),
            confidence: ConfidenceTier::Medium,
        });
    }
    for pair in words.windows(2) {
        if !only_whitespace_between(input, &pair[0], &pair[1]) {
            continue;
        }
        let left = pair[0].text.to_ascii_lowercase();
        let right = pair[1].text.to_ascii_lowercase();
        if grammar_enabled(input, GrammarCategory::RepeatedWords)
            && left == right
            && !matches!(left.as_str(), "had" | "that")
        {
            out.push(Candidate {
                start_byte: pair[0].end_byte,
                end_byte: pair[1].end_byte,
                start_char: pair[0].end_char,
                end_char: pair[1].end_char,
                replacement: String::new(),
                kind: CorrectionChangeKind::Grammar(GrammarCategory::RepeatedWords),
                confidence: ConfidenceTier::High,
            });
        }
        if grammar_enabled(input, GrammarCategory::Articles) {
            let replacement = match (left.as_str(), right.chars().next()) {
                ("a", _) if matches!(right.as_str(), "hour" | "honest" | "honor" | "heir") => {
                    Some("an")
                }
                ("a", Some('a' | 'e' | 'i' | 'o' | 'u'))
                    if !right.starts_with("uni") && !right.starts_with("use") && right != "one" =>
                {
                    Some("an")
                }
                (
                    "an",
                    Some(
                        'b' | 'c' | 'd' | 'f' | 'g' | 'h' | 'j' | 'k' | 'l' | 'm' | 'n' | 'p' | 'q'
                        | 'r' | 's' | 't' | 'v' | 'w' | 'x' | 'y' | 'z',
                    ),
                ) if !matches!(right.as_str(), "hour" | "honest" | "honor" | "heir") => Some("a"),
                ("an", _)
                    if right.starts_with("uni") || right.starts_with("use") || right == "one" =>
                {
                    Some("a")
                }
                _ => None,
            };
            if let Some(mut replacement) =
                replacement.and_then(|value| preserve_case(pair[0].text, value))
            {
                if grammar_enabled(input, GrammarCategory::Capitalization)
                    && is_sentence_start(input, pair[0].start_byte)
                {
                    replacement = capitalize_first(&replacement);
                }
                out.push(word_candidate(
                    &pair[0],
                    replacement,
                    CorrectionChangeKind::Grammar(GrammarCategory::Articles),
                    ConfidenceTier::Medium,
                ));
            }
        }
        if grammar_enabled(input, GrammarCategory::Prepositions) {
            let replacement = match (left.as_str(), right.as_str()) {
                ("depend" | "depends", "of") => Some("on"),
                ("interested", "on") => Some("in"),
                ("listen" | "listening", "on") => Some("to"),
                _ => None,
            };
            if let Some(replacement) =
                replacement.and_then(|value| preserve_case(pair[1].text, value))
            {
                out.push(word_candidate(
                    &pair[1],
                    replacement,
                    CorrectionChangeKind::Grammar(GrammarCategory::Prepositions),
                    ConfidenceTier::Medium,
                ));
            }
        }
        if grammar_enabled(input, GrammarCategory::Homophones) {
            let replacement = match (left.as_str(), right.as_str()) {
                ("your", "welcome" | "right") => Some("you're"),
                ("their", "is" | "are") => Some("there"),
                ("its", "a" | "an") => Some("it's"),
                _ => None,
            };
            if let Some(replacement) =
                replacement.and_then(|value| preserve_case(pair[0].text, value))
            {
                out.push(word_candidate(
                    &pair[0],
                    replacement,
                    CorrectionChangeKind::Grammar(GrammarCategory::Homophones),
                    ConfidenceTier::Medium,
                ));
            }
        }
    }
    if grammar_enabled(input, GrammarCategory::Apostrophes) {
        for word in words {
            let replacement = match word.text.to_ascii_lowercase().as_str() {
                "dont" => Some("don't"),
                "cant" => Some("can't"),
                "wont" => Some("won't"),
                "isnt" => Some("isn't"),
                "arent" => Some("aren't"),
                "wasnt" => Some("wasn't"),
                _ => None,
            };
            if let Some(replacement) = replacement.and_then(|value| preserve_case(word.text, value))
            {
                out.push(word_candidate(
                    word,
                    replacement,
                    CorrectionChangeKind::Grammar(GrammarCategory::Apostrophes),
                    ConfidenceTier::High,
                ));
            }
        }
    }
}

/// Checks both grammar mode and the category's explicit setting.
fn grammar_enabled(input: &CorrectionInput, category: GrammarCategory) -> bool {
    input.mode == CorrectionMode::TyposPlusGrammar
        && input.enabled_grammar_categories.contains(&category)
}

/// Maps supported subject and verb pairs to an agreement correction.
fn agreement_replacement(subject: &str, verb: &str) -> Option<&'static str> {
    match (subject, verb) {
        ("i", "is" | "are") => Some("am"),
        ("i", "has") => Some("have"),
        ("this", "are") | ("he" | "she" | "it", "are") => Some("is"),
        ("this", "were") | ("he" | "she" | "it", "were") => Some("was"),
        ("he" | "she" | "it", "have") => Some("has"),
        ("these" | "those" | "we" | "you" | "they", "is") => Some("are"),
        ("these" | "those" | "we" | "you" | "they", "was") => Some("were"),
        ("these" | "those" | "we" | "you" | "they", "has") => Some("have"),
        _ => None,
    }
}

/// Maps supported auxiliary and verb pairs to a tense correction.
fn tense_replacement(auxiliary: &str, verb: &str) -> Option<&'static str> {
    match (auxiliary, verb) {
        ("have" | "has" | "had", "went") => Some("gone"),
        ("have" | "has" | "had", "saw") => Some("seen"),
        ("have" | "has" | "had", "ate") => Some("eaten"),
        ("have" | "has" | "had", "wrote") => Some("written"),
        ("have" | "has" | "had", "took") => Some("taken"),
        ("have" | "has" | "had", "did") => Some("done"),
        ("did", "went") => Some("go"),
        _ => None,
    }
}

/// Removes spaces directly before supported punctuation marks.
fn spacing_candidates(text: &str, out: &mut Vec<Candidate>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        if !matches!(chars[index].1, ' ' | '\t') {
            index += 1;
            continue;
        }

        let start_index = index;
        while index < chars.len() && matches!(chars[index].1, ' ' | '\t') {
            index += 1;
        }
        if index < chars.len() && matches!(chars[index].1, ',' | '.' | '!' | '?' | ';' | ':') {
            let start_byte = chars[start_index].0;
            let end_byte = chars[index].0;
            out.push(Candidate {
                start_byte,
                end_byte,
                start_char: start_index,
                end_char: index,
                replacement: String::new(),
                kind: CorrectionChangeKind::Grammar(GrammarCategory::Spacing),
                confidence: ConfidenceTier::High,
            });
        }
    }
}

/// Removes a second consecutive mark, excluding ellipses and mixed punctuation.
fn extra_punctuation_candidates(text: &str, out: &mut Vec<Candidate>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for index in 1..chars.len() {
        if chars[index].1 == chars[index - 1].1
            && matches!(chars[index].1, '!' | '?' | ',' | ';' | ':')
        {
            out.push(Candidate {
                start_byte: chars[index].0,
                end_byte: chars.get(index + 1).map_or(text.len(), |entry| entry.0),
                start_char: index,
                end_char: index + 1,
                replacement: String::new(),
                kind: CorrectionChangeKind::Grammar(GrammarCategory::ExtraPunctuation),
                confidence: ConfidenceTier::High,
            });
        }
    }
}

/// Copies a word's byte and character offsets into a candidate edit.
fn word_candidate(
    word: &Word<'_>,
    replacement: String,
    kind: CorrectionChangeKind,
    confidence: ConfidenceTier,
) -> Candidate {
    Candidate {
        start_byte: word.start_byte,
        end_byte: word.end_byte,
        start_char: word.start_char,
        end_char: word.end_char,
        replacement,
        kind,
        confidence,
    }
}

/// Splits alphabetic words and records offsets in bytes and Unicode scalars.
fn words(text: &str) -> Vec<Word<'_>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut result = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if !chars[index].1.is_alphabetic() {
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < chars.len() {
            let character = chars[index].1;
            let apostrophe_inside_word = matches!(character, '\'' | '’')
                && index + 1 < chars.len()
                && chars[index + 1].1.is_alphabetic();
            if !character.is_alphabetic() && !apostrophe_inside_word {
                break;
            }
            index += 1;
        }

        let start_byte = chars[start].0;
        let end_byte = chars.get(index).map_or(text.len(), |entry| entry.0);
        result.push(Word {
            text: &text[start_byte..end_byte],
            start_byte,
            end_byte,
            start_char: start,
            end_char: index,
        });
    }

    result
}

/// Collects explicit terms and detectable names or structured tokens to protect.
fn protected_ranges(input: &CorrectionInput, words: &[Word<'_>]) -> Vec<(usize, usize)> {
    let text = &input.executable_context;
    let mut ranges = Vec::new();

    for term in &input.protected_terms {
        add_exact_matches(text, term, &mut ranges);
    }
    for entry in &input.custom_dictionary {
        add_ascii_case_insensitive_matches(text, entry, &mut ranges);
    }
    for (start, end) in non_whitespace_ranges(text) {
        if looks_protected_chunk(&text[start..end]) {
            ranges.push((start, end));
        }
    }

    for word in words {
        if looks_like_identifier_or_product(word.text)
            || (is_title_case(word.text) && !is_sentence_start(input, word.start_byte))
        {
            ranges.push((word.start_byte, word.end_byte));
        }
    }

    // A title-cased pair is a detectable personal or product name, including at
    // the beginning of a sentence where a single capitalized word is ambiguous.
    for pair in words.windows(2) {
        if is_title_case(pair[0].text)
            && is_title_case(pair[1].text)
            && only_whitespace_between(input, &pair[0], &pair[1])
        {
            ranges.push((pair[0].start_byte, pair[1].end_byte));
        }
    }

    ranges
}

/// Adds whole-term, case-sensitive protected matches.
fn add_exact_matches(text: &str, term: &str, ranges: &mut Vec<(usize, usize)>) {
    if term.is_empty() {
        return;
    }
    ranges.extend(text.match_indices(term).filter_map(|(start, matched)| {
        let end = start + matched.len();
        has_term_boundaries(text, start, end, term).then_some((start, end))
    }));
}

/// Adds whole-term dictionary matches regardless of ASCII case.
fn add_ascii_case_insensitive_matches(text: &str, term: &str, ranges: &mut Vec<(usize, usize)>) {
    if term.is_empty() {
        return;
    }
    let folded_text = text.to_ascii_lowercase();
    let folded_term = term.to_ascii_lowercase();
    ranges.extend(
        folded_text
            .match_indices(&folded_term)
            .filter_map(|(start, matched)| {
                let end = start + matched.len();
                has_term_boundaries(text, start, end, term).then_some((start, end))
            }),
    );
}

/// Prevents a protected word from matching inside a larger identifier.
fn has_term_boundaries(text: &str, start: usize, end: usize, term: &str) -> bool {
    let starts_as_word = term.chars().next().is_some_and(is_identifier_character);
    let ends_as_word = term
        .chars()
        .next_back()
        .is_some_and(is_identifier_character);
    let before_is_word = text[..start]
        .chars()
        .next_back()
        .is_some_and(is_identifier_character);
    let after_is_word = text[end..]
        .chars()
        .next()
        .is_some_and(is_identifier_character);

    (!starts_as_word || !before_is_word) && (!ends_as_word || !after_is_word)
}

/// Treats letters, digits, and underscores as identifier boundaries.
fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Returns byte ranges of contiguous non-whitespace chunks.
fn non_whitespace_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (byte, character) in text.char_indices() {
        if character.is_whitespace() {
            if let Some(chunk_start) = start.take() {
                ranges.push((chunk_start, byte));
            }
        } else if start.is_none() {
            start = Some(byte);
        }
    }
    if let Some(chunk_start) = start {
        ranges.push((chunk_start, text.len()));
    }
    ranges
}

/// Detects URLs, addresses, paths, handles, and code-like chunks.
fn looks_protected_chunk(chunk: &str) -> bool {
    let trimmed = chunk.trim_matches(|character: char| {
        matches!(
            character,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '"'
                | '\''
                | ','
                | ';'
                | '!'
                | '?'
                | '.'
        )
    });
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    let is_url =
        lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.");
    let is_email = trimmed
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    let is_path = trimmed.starts_with("\\\\")
        || trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed.contains('\\')
        || trimmed
            .as_bytes()
            .get(1..3)
            .is_some_and(|pair| pair == b":\\" || pair == b":/")
        || (trimmed.contains('/') && (trimmed.contains('.') || trimmed.matches('/').count() > 1));
    let is_dotted_resource = trimmed.rsplit_once('.').is_some_and(|(stem, suffix)| {
        !stem.is_empty()
            && (1..=12).contains(&suffix.len())
            && suffix
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    });
    let is_handle = (trimmed.starts_with('@') || trimmed.starts_with('#'))
        && trimmed[1..]
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_');
    let is_code = trimmed.contains('_')
        || trimmed.contains('-')
        || trimmed.contains("::")
        || trimmed.contains("->")
        || trimmed.contains("=>")
        || trimmed.starts_with('`')
        || chunk.contains("()");

    is_url || is_email || is_path || is_dotted_resource || is_handle || is_code
}

/// Detects non-English words, mixed case names, and identifier-like words.
fn looks_like_identifier_or_product(word: &str) -> bool {
    if !word.is_ascii() {
        return true;
    }
    let has_lower = word.bytes().any(|byte| byte.is_ascii_lowercase());
    let has_upper_after_first = word.bytes().skip(1).any(|byte| byte.is_ascii_uppercase());
    let has_digit = word.bytes().any(|byte| byte.is_ascii_digit());
    let all_upper = word.len() > 1
        && word
            .bytes()
            .all(|byte| !byte.is_ascii_alphabetic() || byte.is_ascii_uppercase());
    (has_lower && has_upper_after_first) || has_digit || all_upper
}

/// Checks for one leading uppercase character followed by lowercase letters.
fn is_title_case(word: &str) -> bool {
    let mut characters = word.chars();
    characters.next().is_some_and(char::is_uppercase)
        && characters.all(|character| !character.is_alphabetic() || character.is_lowercase())
}

/// Uses read-only preceding context to identify a sentence boundary.
fn is_sentence_start(input: &CorrectionInput, start_byte: usize) -> bool {
    let executable_prefix = &input.executable_context[..start_byte];
    let previous = previous_meaningful(executable_prefix)
        .or_else(|| previous_meaningful(&input.informative_context));
    previous.is_none_or(|character| matches!(character, '.' | '!' | '?'))
}

/// Skips trailing whitespace and closing marks when finding preceding text.
fn previous_meaningful(text: &str) -> Option<char> {
    text.chars().rev().find(|character| {
        !character.is_whitespace() && !matches!(character, '"' | '\'' | '”' | '’' | ')' | ']' | '}')
    })
}

/// Requires adjacent rule words to have no intervening content.
fn only_whitespace_between(input: &CorrectionInput, left: &Word<'_>, right: &Word<'_>) -> bool {
    input.executable_context[left.end_byte..right.start_byte]
        .chars()
        .all(char::is_whitespace)
}

/// Matches lower, upper, or title case and rejects ambiguous casing.
fn preserve_case(original: &str, replacement: &str) -> Option<String> {
    if original
        .bytes()
        .all(|byte| !byte.is_ascii_alphabetic() || byte.is_ascii_lowercase())
    {
        Some(replacement.to_owned())
    } else if original
        .bytes()
        .all(|byte| !byte.is_ascii_alphabetic() || byte.is_ascii_uppercase())
    {
        Some(replacement.to_ascii_uppercase())
    } else if is_title_case(original) {
        Some(capitalize_first(replacement))
    } else {
        None
    }
}

/// Uppercases the first Unicode character without changing the rest.
fn capitalize_first(text: &str) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect()
}

/// Checks whether a candidate overlaps any protected byte range.
fn intersects_any(start: usize, end: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(other_start, other_end)| ranges_intersect(start, end, *other_start, *other_end))
}

/// Tests overlap between two half-open byte ranges.
fn ranges_intersect(start: usize, end: usize, other_start: usize, other_end: usize) -> bool {
    start < other_end && other_start < end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::correction::{
        ConfidenceBehaviorSettings, LanguageInfo, MixedLanguagePolicy, TriggerType,
    };

    fn input(text: &str, mode: CorrectionMode) -> CorrectionInput {
        CorrectionInput {
            informative_context: String::new(),
            executable_context: text.to_owned(),
            mode,
            enabled_grammar_categories: Vec::new(),
            language_info: LanguageInfo {
                primary_language: Some("en-US".to_owned()),
                detected_languages: vec!["en-US".to_owned()],
            },
            mixed_language_policy: MixedLanguagePolicy::PreserveNonPrimary,
            custom_dictionary: Vec::new(),
            protected_terms: Vec::new(),
            trigger_type: TriggerType::ManualShortcut,
            confidence_behavior: ConfidenceBehaviorSettings {
                high: ConfidenceBehavior::Silent,
                medium: ConfidenceBehavior::Suggestion,
                low: ConfidenceBehavior::DoNothing,
            },
        }
    }

    #[test]
    fn typos_only_changes_only_clear_misspellings_and_preserves_layout_and_case() {
        let output = correct(&input("Teh  wierd, teh!", CorrectionMode::TyposOnly));

        assert_eq!(output.corrected_executable_text, "The  weird, the!");
        assert_eq!(output.confidence, ConfidenceTier::High);
        assert_eq!(output.changes.as_ref().unwrap().len(), 3);
        assert_eq!(output.changes.as_ref().unwrap()[1].start_char, 5);
    }

    #[test]
    fn custom_dictionary_and_protected_terms_block_candidates() {
        let mut request = input("teh wierd", CorrectionMode::TyposOnly);
        request.custom_dictionary.push("teh".to_owned());
        request.protected_terms.push("wierd".to_owned());

        let output = correct(&request);

        assert_eq!(output.corrected_executable_text, request.executable_context);
        assert_eq!(
            output.no_change_reason,
            Some(NoChangeReason::AllCandidatesProtected)
        );
    }

    #[test]
    fn default_protections_cover_structured_and_detectable_named_tokens() {
        let request = input(
            "teh@example.com https://site.test/teh C:\\teh\\file teh.txt @teh #teh teh_value teh-value teh() AutoTeh John Teh",
            CorrectionMode::TyposOnly,
        );

        let output = correct(&request);

        assert_eq!(output.corrected_executable_text, request.executable_context);
        assert_eq!(
            output.no_change_reason,
            Some(NoChangeReason::AllCandidatesProtected)
        );
    }

    #[test]
    fn grammar_mode_applies_only_enabled_categories() {
        let mut request = input("i have went home .", CorrectionMode::TyposPlusGrammar);
        request.enabled_grammar_categories =
            vec![GrammarCategory::Capitalization, GrammarCategory::Spacing];

        let output = correct(&request);

        assert_eq!(output.corrected_executable_text, "I have went home.");
        assert!(output.changes.as_ref().unwrap().iter().all(|change| {
            matches!(
                change.kind,
                CorrectionChangeKind::Grammar(GrammarCategory::Capitalization)
                    | CorrectionChangeKind::Grammar(GrammarCategory::Spacing)
            )
        }));
    }

    #[test]
    fn agreement_and_tense_rules_are_category_gated() {
        let mut request = input(
            "These is ready. They has went.",
            CorrectionMode::TyposPlusGrammar,
        );
        request.enabled_grammar_categories =
            vec![GrammarCategory::Agreement, GrammarCategory::Tense];

        let output = correct(&request);

        assert_eq!(
            output.corrected_executable_text,
            "These are ready. They have gone."
        );
    }

    #[test]
    fn suggested_grammar_rules_require_their_categories() {
        let cases = [
            (
                GrammarCategory::MissingPunctuation,
                "we are ready",
                "we are ready.",
            ),
            (GrammarCategory::ExtraPunctuation, "ready!!", "ready!"),
            (GrammarCategory::RepeatedWords, "the the book", "the book"),
            (GrammarCategory::Articles, "a apple", "an apple"),
            (
                GrammarCategory::Prepositions,
                "depend of us",
                "depend on us",
            ),
            (GrammarCategory::Spacing, "ready !", "ready!"),
            (GrammarCategory::Apostrophes, "dont go", "don't go"),
            (
                GrammarCategory::Homophones,
                "your welcome",
                "you're welcome",
            ),
        ];
        for (category, original, expected) in cases {
            let mut request = input(original, CorrectionMode::TyposPlusGrammar);
            assert_eq!(correct(&request).corrected_executable_text, original);
            request.enabled_grammar_categories = vec![category];
            let output = correct(&request);
            assert_eq!(output.corrected_executable_text, expected, "{category:?}");
            assert!(output
                .changes
                .unwrap()
                .iter()
                .all(|change| { change.kind == CorrectionChangeKind::Grammar(category) }));
        }
    }

    #[test]
    fn article_and_capitalization_combine_only_when_both_enabled() {
        let mut request = input("a apple", CorrectionMode::TyposPlusGrammar);
        request.enabled_grammar_categories =
            vec![GrammarCategory::Articles, GrammarCategory::Capitalization];
        assert_eq!(correct(&request).corrected_executable_text, "An apple");
        request.enabled_grammar_categories = vec![GrammarCategory::Articles];
        assert_eq!(correct(&request).corrected_executable_text, "an apple");
        request.executable_context = "an university".into();
        assert_eq!(correct(&request).corrected_executable_text, "a university");
    }

    #[test]
    fn typos_only_ignores_grammar_categories_even_if_input_is_inconsistent() {
        let mut request = input("i is ready .", CorrectionMode::TyposOnly);
        request.enabled_grammar_categories = vec![
            GrammarCategory::Agreement,
            GrammarCategory::Capitalization,
            GrammarCategory::Spacing,
        ];

        let output = correct(&request);

        assert_eq!(output.corrected_executable_text, request.executable_context);
        assert_eq!(
            output.no_change_reason,
            Some(NoChangeReason::NoCorrectionNeeded)
        );
    }

    #[test]
    fn confidence_behavior_can_suppress_medium_candidates() {
        let mut request = input("alot", CorrectionMode::TyposOnly);
        request.confidence_behavior.medium = ConfidenceBehavior::DoNothing;

        let output = correct(&request);

        assert_eq!(output.corrected_executable_text, "alot");
        assert_eq!(
            output.no_change_reason,
            Some(NoChangeReason::ConfidenceBelowConfiguredBehavior)
        );
    }

    #[test]
    fn non_english_input_is_left_unchanged() {
        let mut request = input("teh", CorrectionMode::TyposOnly);
        request.language_info.primary_language = Some("es".to_owned());

        let output = correct(&request);

        assert_eq!(output.corrected_executable_text, "teh");
        assert_eq!(
            output.no_change_reason,
            Some(NoChangeReason::UnsupportedLanguage)
        );
    }

    #[test]
    fn reports_unicode_scalar_offsets() {
        let output = correct(&input("🙂 teh", CorrectionMode::TyposOnly));
        let change = &output.changes.as_ref().unwrap()[0];

        assert_eq!((change.start_char, change.end_char), (2, 5));
    }
}
