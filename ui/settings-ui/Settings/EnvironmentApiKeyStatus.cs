namespace AutoFix.SettingsUi.Settings;

public sealed class EnvironmentApiKeyStatus : IApiKeyStatus
{
    public bool HasConfiguredApiKey(AppConfig config)
    {
        var providerKey = $"AUTOFIX_{config.Api.ProviderPreset.ToUpperInvariant()}_API_KEY";
        return HasValue(providerKey) || HasValue("AUTOFIX_API_KEY");
    }

    private static bool HasValue(string variable) =>
        !string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable(variable));
}
