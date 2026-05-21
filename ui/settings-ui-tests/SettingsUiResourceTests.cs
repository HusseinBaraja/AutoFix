using System.Xml.Linq;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class SettingsUiResourceTests
{
    private static readonly XNamespace Presentation = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";
    private static readonly XNamespace Xaml = "http://schemas.microsoft.com/winfx/2006/xaml";

    [TestMethod]
    public void TextBoxesUseStockTemplateWithExplicitPlaceholders()
    {
        var controls = LoadXaml("Resources", "SettingsControls.xaml");
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsFalse(controls
            .Descendants(Presentation + "ControlTemplate")
            .Any(template => (string?)template.Attribute("TargetType") == "TextBox"));
        Assert.IsTrue(window
            .Descendants(Presentation + "TextBlock")
            .Any(text => (string?)text.Attribute("Text") == "Search settings"));
        Assert.IsTrue(window
            .Descendants(Presentation + "TextBlock")
            .Any(text => (string?)text.Attribute("Text") == "{Binding Description}"));
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

    [TestMethod]
    public void SettingsContentIsConstrainedToScrollViewport()
    {
        var window = LoadXaml("MainWindow.xaml");
        var scrollViewer = window
            .Descendants(Presentation + "ScrollViewer")
            .Single(element => (string?)element.Attribute(Xaml + "Name") == "SettingsScrollViewer");
        var settingsItems = window
            .Descendants(Presentation + "ItemsControl")
            .Single(element => (string?)element.Attribute("ItemsSource") == "{Binding SelectedSection.Settings}");

        var content = scrollViewer.Element(Presentation + "StackPanel");

        Assert.AreEqual("Disabled", (string?)scrollViewer.Attribute("HorizontalScrollBarVisibility"));
        Assert.AreEqual("{Binding ViewportWidth, ElementName=SettingsScrollViewer}", (string?)content?.Attribute("Width"));
        Assert.AreEqual("{Binding ViewportWidth, ElementName=SettingsScrollViewer}", (string?)settingsItems.Attribute("Width"));
    }

    [TestMethod]
    public void ToggleSettingsAreRightAligned()
    {
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsTrue(window
            .Descendants(Presentation + "CheckBox")
            .Any(checkBox => (string?)checkBox.Attribute("Style") == "{StaticResource ToggleSwitch}"
                && (string?)checkBox.Attribute("HorizontalAlignment") == "Right"));
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
