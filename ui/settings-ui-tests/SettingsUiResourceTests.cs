using System.Xml.Linq;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class SettingsUiResourceTests
{
    private static readonly XNamespace Presentation = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";
    private static readonly XNamespace Xaml = "http://schemas.microsoft.com/winfx/2006/xaml";

    [TestMethod]
    public void TextBoxTemplateRendersPlaceholderAndTypedText()
    {
        var controls = LoadXaml("Resources", "SettingsControls.xaml");
        var textBoxTemplate = controls
            .Descendants(Presentation + "ControlTemplate")
            .Single(template => (string?)template.Attribute("TargetType") == "TextBox");

        Assert.IsTrue(textBoxTemplate
            .Descendants(Presentation + "ScrollViewer")
            .Any(viewer => (string?)viewer.Attribute(Xaml + "Name") == "PART_ContentHost"));
        Assert.IsTrue(textBoxTemplate
            .Descendants(Presentation + "TextBlock")
            .Any(text => (string?)text.Attribute(Xaml + "Name") == "PlaceholderText"
                && (string?)text.Attribute("Text") == "{TemplateBinding Tag}"));
    }

    [TestMethod]
    public void ComboBoxTemplateUsesSelectionBoxTemplateForDisplayMemberPath()
    {
        var controls = LoadXaml("Resources", "SettingsControls.xaml");
        var comboBoxTemplate = controls
            .Descendants(Presentation + "ControlTemplate")
            .Single(template => (string?)template.Attribute("TargetType") == "ComboBox");

        Assert.IsTrue(comboBoxTemplate
            .Descendants(Presentation + "ToggleButton")
            .Any(button => (string?)button.Attribute("Content") == "{TemplateBinding SelectionBoxItem}"
                && (string?)button.Attribute("ContentTemplate") == "{TemplateBinding SelectionBoxItemTemplate}"
                && (string?)button.Attribute("ContentStringFormat") == "{TemplateBinding SelectionBoxItemStringFormat}"));
    }

    [TestMethod]
    public void SettingControlsFitInsideTheirColumn()
    {
        var window = LoadXaml("MainWindow.xaml");
        var columnWidth = window
            .Descendants(Presentation + "ColumnDefinition")
            .Select(column => (string?)column.Attribute("Width"))
            .Single(width => width == "280");

        Assert.AreEqual("280", columnWidth);
        Assert.IsFalse(window
            .Descendants()
            .Any(element => ((string?)element.Attribute("Width")) == "250"));
    }

    private static XElement LoadXaml(params string[] pathParts)
    {
        var repoRoot = FindRepoRoot(new DirectoryInfo(AppContext.BaseDirectory));
        var path = Path.Combine(
            [repoRoot.FullName, "ui", "settings-ui", .. pathParts]);

        return XElement.Load(path);
    }

    private static DirectoryInfo FindRepoRoot(DirectoryInfo start)
    {
        for (var current = start; current is not null; current = current.Parent)
        {
            if (File.Exists(Path.Combine(current.FullName, "AGENTS.md")))
            {
                return current;
            }
        }

        throw new DirectoryNotFoundException("Could not find repository root.");
    }
}
