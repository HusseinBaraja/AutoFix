using System.Xml.Linq;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class SettingsUiResourceTests
{
    private static readonly XNamespace Presentation = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";
    private static readonly XNamespace Xaml = "http://schemas.microsoft.com/winfx/2006/xaml";

    [TestMethod]
    public void TextBoxesUseStockTemplateInsideRoundedShells()
    {
        var controls = LoadXaml("Resources", "SettingsControls.xaml");
        var window = LoadXaml("MainWindow.xaml");
        var searchBox = window
            .Descendants(Presentation + "TextBox")
            .Single(textBox => (string?)textBox.Attribute(Xaml + "Name") == "SearchBox");

        Assert.IsFalse(controls
            .Descendants(Presentation + "ControlTemplate")
            .Any(template => (string?)template.Attribute("TargetType") == "TextBox"));
        Assert.IsTrue(controls
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "InputShell"));
        Assert.IsTrue(window
            .Descendants(Presentation + "Border")
            .Any(border => (string?)border.Attribute("Style") == "{StaticResource InputShell}"));
        Assert.IsTrue(window
            .Descendants(Presentation + "TextBlock")
            .Any(text => (string?)text.Attribute("Text") == "Search settings"));
        Assert.AreEqual("Search settings", (string?)searchBox.Attribute("AutomationProperties.Name"));
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
    public void RoundedSurfacesUseSharedCornerRadius()
    {
        var window = LoadXaml("MainWindow.xaml");
        var controls = LoadXaml("Resources", "SettingsControls.xaml");
        var chrome = LoadXaml("Resources", "SettingsChrome.xaml");
        var tables = LoadXaml("Resources", "SettingsTables.xaml");

        Assert.AreEqual("Resources/SettingsTheme.xaml", window
            .Descendants(Presentation + "ResourceDictionary")
            .Select(dictionary => (string?)dictionary.Attribute("Source"))
            .First(source => source is not null));
        Assert.AreEqual("8", (string?)LoadXaml("Resources", "SettingsTheme.xaml")
            .Elements(Presentation + "CornerRadius")
            .Single(radius => (string?)radius.Attribute(Xaml + "Key") == "SettingsCornerRadius"));
        Assert.IsFalse(new[] { controls, chrome, tables }
            .SelectMany(resource => resource.Descendants())
            .Any(element => (string?)element.Attribute("CornerRadius") is "6" or "7" or "8"));
        Assert.IsTrue(new[] { controls, chrome, tables }
            .SelectMany(resource => resource.Descendants())
            .Any(element => (string?)element.Attribute("CornerRadius") == "{StaticResource SettingsCornerRadius}"));
    }

    [TestMethod]
    public void WindowIconUsesDeferredApplicationResourceLookup()
    {
        var window = LoadXaml("MainWindow.xaml");

        Assert.AreEqual("{DynamicResource AutoFixLogoImage}", (string?)window.Attribute("Icon"));
    }

    [TestMethod]
    public void TablesRenderInsideRoundedShells()
    {
        var tables = LoadXaml("Resources", "SettingsTables.xaml");
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsTrue(tables
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "TableShell"));
        Assert.IsTrue(window
            .Descendants(Presentation + "Border")
            .Any(border => (string?)border.Attribute("Style") == "{StaticResource TableShell}"));
        Assert.IsTrue(tables
            .Descendants(Presentation + "Style")
            .Where(style => (string?)style.Attribute("TargetType") == "DataGrid")
            .Descendants(Presentation + "Setter")
            .Any(setter => (string?)setter.Attribute("Property") == "BorderThickness"
                && (string?)setter.Attribute("Value") == "0"));
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
