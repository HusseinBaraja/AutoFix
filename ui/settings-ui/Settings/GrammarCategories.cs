namespace AutoFix.SettingsUi.Settings;

public sealed record GrammarCategoryOption(string Value, string Label, string Description);

public static class GrammarCategories
{
    public static IReadOnlyList<GrammarCategoryOption> All { get; } =
    [
        new("capitalization", "Capitalization", "Sentence starts and the pronoun I."),
        new("missing_punctuation", "Missing punctuation", "Missing sentence-ending marks."),
        new("extra_punctuation", "Extra punctuation", "Repeated punctuation marks."),
        new("repeated_words", "Repeated words", "Accidentally repeated adjacent words."),
        new("agreement", "Subject-verb agreement", "Agreement between common subjects and verbs."),
        new("articles", "Articles: a/an/the", "Incorrect articles before common words."),
        new("prepositions", "Prepositions", "Common incorrect prepositions."),
        new("spacing", "Spacing", "Spaces around punctuation."),
        new("apostrophes", "Apostrophes and contractions", "Missing apostrophes in common contractions."),
        new("homophones", "Common homophones", "Context-dependent words such as your and you're."),
        new("tense", "Verb tense", "Common auxiliary and past-participle mistakes."),
        new("clarity", "Clarity (API)", "Optional clarity edits with an API engine."),
        new("word_order", "Word order (API)", "Optional word-order edits with an API engine."),
    ];
}
