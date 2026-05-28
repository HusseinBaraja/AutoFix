using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;

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
        var query = Query?.Trim() ?? "";
        if (text.Length == 0)
        {
            return;
        }

        if (query.Length == 0)
        {
            Inlines.Add(new Run(text));
            return;
        }

        var cursor = 0;
        while (cursor < text.Length)
        {
            var match = text.IndexOf(query, cursor, StringComparison.OrdinalIgnoreCase);
            if (match < 0)
            {
                Inlines.Add(new Run(text[cursor..]));
                break;
            }

            if (match > cursor)
            {
                Inlines.Add(new Run(text[cursor..match]));
            }

            Inlines.Add(new Run(text.Substring(match, query.Length))
            {
                Background = Brushes.Gold,
                Foreground = Brushes.Black,
            });
            cursor = match + query.Length;
        }
    }
}
