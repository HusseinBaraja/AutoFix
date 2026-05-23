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
            .Single(width => width == "320");

        Assert.AreEqual("320", columnWidth);
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

    [TestMethod]
    public void HeaderDoesNotExposeManualSaveOrReloadButtons()
    {
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsFalse(window
            .Descendants(Presentation + "Button")
            .Any(button => (string?)button.Attribute("Content") is "Save" or "Reload"));
    }

    [TestMethod]
    public void SettingTextBoxesUpdateOnEveryChange()
    {
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsTrue(window
            .Descendants(Presentation + "TextBox")
            .Any(textBox => (string?)textBox.Attribute("Text") == "{Binding TextValue, UpdateSourceTrigger=PropertyChanged}"));
    }

    [TestMethod]
    public void ToggleSwitchExposesFocusAndDisabledStates()
    {
        var chrome = LoadXaml("Resources", "SettingsChrome.xaml");
        var toggleStyle = chrome
            .Descendants(Presentation + "Style")
            .Single(style => (string?)style.Attribute(Xaml + "Key") == "ToggleSwitch");

        AssertTriggerTargets(toggleStyle, "IsKeyboardFocused", "Track", "BorderBrush");
        AssertTriggerTargets(toggleStyle, "IsKeyboardFocused", "Track", "BorderThickness");
        AssertTriggerTargets(toggleStyle, "IsEnabled", "Track", "Opacity");
        AssertTriggerTargets(toggleStyle, "IsEnabled", "Thumb", "Opacity");
        Assert.IsTrue(toggleStyle
            .Descendants(Presentation + "Setter")
            .Any(setter => (string?)setter.Attribute("Property") == "Cursor"
                && (string?)setter.Attribute("Value") == "Hand"));
    }

    [TestMethod]
    public void SidebarItemExposesKeyboardFocusState()
    {
        var chrome = LoadXaml("Resources", "SettingsChrome.xaml");
        var sidebarStyle = chrome
            .Descendants(Presentation + "Style")
            .Single(style => (string?)style.Attribute(Xaml + "Key") == "SidebarItem");

        AssertTriggerTargets(sidebarStyle, "IsKeyboardFocused", "Root", "BorderBrush");
        AssertTriggerTargets(sidebarStyle, "IsKeyboardFocused", "Root", "BorderThickness");
    }

    [TestMethod]
    public void TableRowsExposeKeyboardFocusState()
    {
        var tables = LoadXaml("Resources", "SettingsTables.xaml");
        var rowStyle = tables
            .Descendants(Presentation + "Style")
            .Single(style => (string?)style.Attribute("TargetType") == "DataGridRow");

        AssertStyleTriggerSets(rowStyle, "IsKeyboardFocusWithin", "Background");
        AssertStyleTriggerSets(rowStyle, "IsKeyboardFocusWithin", "BorderBrush");
        AssertStyleSetter(rowStyle, "BorderThickness", "1");
        AssertStyleTriggerDoesNotSet(rowStyle, "IsKeyboardFocusWithin", "BorderThickness");
    }

    [TestMethod]
    public void HotkeyRecorderResourcesExist()
    {
        var recorder = LoadXaml("Resources", "HotkeyRecorder.xaml");
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsTrue(recorder
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "HotkeyRecorderWell"));
        Assert.IsTrue(recorder
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "HotkeyConflictWarning"));
        Assert.IsTrue(recorder
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "KeycapPill"));
        Assert.IsTrue(recorder
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "HotkeyClearButton"));
        Assert.IsTrue(recorder
            .Descendants(Presentation + "Style")
            .Any(style => (string?)style.Attribute(Xaml + "Key") == "HotkeyDefaultLink"));
        Assert.IsTrue(window
            .Descendants(Presentation + "ResourceDictionary")
            .Any(dictionary => (string?)dictionary.Attribute("Source") == "Resources/HotkeyRecorder.xaml"));
    }

    [TestMethod]
    public void HotkeyRecorderUIElementsExistInWindow()
    {
        var window = LoadXaml("MainWindow.xaml");

        Assert.IsTrue(window
            .Descendants(Presentation + "Border")
            .Any(border => (string?)border.Attribute("Style") == "{StaticResource HotkeyRecorderWell}"));
        Assert.IsTrue(window
            .Descendants(Presentation + "Border")
            .Any(border => (string?)border.Attribute("Style") == "{StaticResource HotkeyConflictWarning}"));
        Assert.IsTrue(window
            .Descendants(Presentation + "TextBlock")
            .Any(text => ((string?)text.Attribute("Text"))?.Contains("Recording") == true));
        Assert.AreEqual("Window_Deactivated", (string?)window.Attribute("Deactivated"));
    }

    [TestMethod]
    public void HotkeyPillsDoNotBindItemsSource()
    {
        var window = LoadXaml("MainWindow.xaml");
        var hotkeyPills = window
            .Descendants(Presentation + "ItemsControl")
            .Single(control => (string?)control.Attribute("Loaded") == "HotkeyPills_Loaded");

        Assert.IsNull(hotkeyPills.Attribute("ItemsSource"));
        Assert.AreEqual("{Binding Hotkey}", (string?)hotkeyPills.Attribute("Tag"));
    }

    [TestMethod]
    public void HotkeyPillsLetRecorderWellReceiveClicks()
    {
        var window = LoadXaml("MainWindow.xaml");
        var hotkeyPills = window
            .Descendants(Presentation + "ItemsControl")
            .Single(control => (string?)control.Attribute("Loaded") == "HotkeyPills_Loaded");
        var hotkeyButtons = window
            .Descendants(Presentation + "Button")
            .Where(button => (string?)button.Attribute("Style") is "{StaticResource HotkeyClearButton}" or "{StaticResource HotkeyDefaultLink}")
            .ToList();

        Assert.AreEqual("False", (string?)hotkeyPills.Attribute("IsHitTestVisible"));
        Assert.IsTrue(hotkeyButtons.Count > 0);
        Assert.IsTrue(hotkeyButtons.All(button => button.Attribute("IsHitTestVisible") is null));
    }

    private static void AssertTriggerTargets(
        XElement style,
        string property,
        string targetName,
        string setterProperty)
    {
        Assert.IsTrue(style
            .Descendants(Presentation + "Trigger")
            .Where(trigger => (string?)trigger.Attribute("Property") == property)
            .Descendants(Presentation + "Setter")
            .Any(setter => (string?)setter.Attribute("TargetName") == targetName
                && (string?)setter.Attribute("Property") == setterProperty));
    }

    private static void AssertStyleTriggerSets(
        XElement style,
        string property,
        string setterProperty)
    {
        Assert.IsTrue(style
            .Descendants(Presentation + "Trigger")
            .Where(trigger => (string?)trigger.Attribute("Property") == property)
            .Descendants(Presentation + "Setter")
            .Any(setter => (string?)setter.Attribute("Property") == setterProperty));
    }

    private static void AssertStyleTriggerDoesNotSet(
        XElement style,
        string property,
        string setterProperty)
    {
        Assert.IsFalse(style
            .Descendants(Presentation + "Trigger")
            .Where(trigger => (string?)trigger.Attribute("Property") == property)
            .Descendants(Presentation + "Setter")
            .Any(setter => (string?)setter.Attribute("Property") == setterProperty));
    }

    private static void AssertStyleSetter(
        XElement style,
        string setterProperty,
        string value)
    {
        Assert.IsTrue(style
            .Elements(Presentation + "Setter")
            .Any(setter => (string?)setter.Attribute("Property") == setterProperty
                && (string?)setter.Attribute("Value") == value));
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
