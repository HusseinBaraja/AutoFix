using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;
using AutoFix.SettingsUi.Settings;

namespace AutoFix.SettingsUi.Controls;

public sealed class HighlightTextBlock : TextBlock
{
    public static readonly DependencyProperty HighlightTextProperty =
        DependencyProperty.Register(
            nameof(HighlightText),
            typeof(string),
            typeof(HighlightTextBlock),
            new PropertyMetadata("", OnTextChanged));

    public static readonly DependencyProperty QueryProperty =
        DependencyProperty.Register(
            nameof(Query),
            typeof(string),
            typeof(HighlightTextBlock),
            new PropertyMetadata("", OnTextChanged));

    public string HighlightText
    {
        get => (string)GetValue(HighlightTextProperty);
        set => SetValue(HighlightTextProperty, value);
    }

    public string Query
    {
        get => (string)GetValue(QueryProperty);
        set => SetValue(QueryProperty, value);
    }

    private static void OnTextChanged(DependencyObject source, DependencyPropertyChangedEventArgs e)
    {
        ((HighlightTextBlock)source).RenderText();
    }

    private void RenderText()
    {
        Inlines.Clear();

        var text = HighlightText ?? "";
        var query = new SettingsSearchQuery(Query ?? "");
        if (text.Length == 0)
        {
            return;
        }

        if (query.Terms.Count == 0)
        {
            Inlines.Add(new Run(text));
            return;
        }

        // Merge overlapping terms so multi-word and path-style queries highlight consistently.
        var highlighted = new bool[text.Length];
        foreach (var term in query.Terms)
        {
            for (var start = 0; start < text.Length;)
            {
                var match = text.IndexOf(term, start, StringComparison.OrdinalIgnoreCase);
                if (match < 0) break;
                Array.Fill(highlighted, true, match, term.Length);
                start = match + 1;
            }
        }
        for (var cursor = 0; cursor < text.Length;)
        {
            var start = cursor;
            var highlight = highlighted[cursor++];
            while (cursor < text.Length && highlighted[cursor] == highlight) cursor++;
            var run = new Run(text[start..cursor]);
            if (highlight)
            {
                run.Background = Brushes.Gold;
                run.Foreground = Brushes.Black;
            }
            Inlines.Add(run);
        }
    }
}
