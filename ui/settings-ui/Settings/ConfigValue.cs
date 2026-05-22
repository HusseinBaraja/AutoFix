using System.Globalization;
using System.IO;

namespace AutoFix.SettingsUi.Settings;

public static class ConfigValue
{
    public static string Join(IReadOnlyCollection<string> values) => string.Join(", ", values);

    public static List<string> Split(string? value) =>
        (value ?? string.Empty).Split(',', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries).ToList();

    public static string Text(string? value) => value ?? "";

    public static int Int(string value, string field)
    {
        if (!int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsed))
        {
            throw new InvalidDataException($"{field}: must be a whole number");
        }

        return parsed;
    }

    public static long Long(string value, string field)
    {
        if (!long.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsed))
        {
            throw new InvalidDataException($"{field}: must be a whole number");
        }

        return parsed;
    }

    public static double Double(string value, string field)
    {
        if (!double.TryParse(value, NumberStyles.Float | NumberStyles.AllowThousands, CultureInfo.InvariantCulture, out var parsed))
        {
            throw new InvalidDataException($"{field}: must be a number");
        }

        return parsed;
    }

    public static int? OptionalInt(string value, string field)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        return Int(value, field);
    }
}
