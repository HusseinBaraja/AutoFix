namespace AutoFix.Core.Llm;

public enum ApiKind
{
    /// <summary>OpenAI-compatible <c>/chat/completions</c>.</summary>
    OpenAi,
    /// <summary>Anthropic <c>/v1/messages</c>.</summary>
    Anthropic,
}

public sealed record ProviderPreset(
    string Id,
    string Name,
    ApiKind Kind,
    string BaseUrl,
    string DefaultModel,
    string? KeyUrl,
    bool NeedsKey = true,
    string? KeyHint = null)
{
    public bool IsCustom => Id == ProviderCatalog.CustomId;

    public override string ToString() => Name;
}

public static class ProviderCatalog
{
    public const string CustomId = "custom";
    public const string DefaultId = "deepseek";

    /// <summary>Default models favor fast, non-reasoning models: typing corrections need low latency.</summary>
    public static readonly IReadOnlyList<ProviderPreset> All =
    [
        new("deepseek", "DeepSeek", ApiKind.OpenAi, "https://api.deepseek.com", "deepseek-chat",
            "https://platform.deepseek.com/api_keys"),
        new("openai", "OpenAI", ApiKind.OpenAi, "https://api.openai.com/v1", "gpt-4.1-mini",
            "https://platform.openai.com/api-keys", KeyHint: "sk-proj-"),
        new("anthropic", "Anthropic Claude", ApiKind.Anthropic, "https://api.anthropic.com", "claude-haiku-5-5",
            "https://console.anthropic.com/settings/keys", KeyHint: "sk-ant-"),
        new("gemini", "Google Gemini", ApiKind.OpenAi, "https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.5-flash-lite",
            "https://aistudio.google.com/apikey", KeyHint: "AIza"),
        new("groq", "Groq", ApiKind.OpenAi, "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile",
            "https://console.groq.com/keys", KeyHint: "gsk_"),
        new("openrouter", "OpenRouter", ApiKind.OpenAi, "https://openrouter.ai/api/v1", "openai/gpt-4.1-mini",
            "https://openrouter.ai/keys", KeyHint: "sk-or-"),
        new("mistral", "Mistral", ApiKind.OpenAi, "https://api.mistral.ai/v1", "mistral-small-latest",
            "https://console.mistral.ai/api-keys"),
        new("ollama", "Ollama (on this PC)", ApiKind.OpenAi, "http://localhost:11434/v1", "llama3.2",
            null, NeedsKey: false),
        new(CustomId, "Other (OpenAI-compatible)", ApiKind.OpenAi, "", "",
            null),
    ];

    public static ProviderPreset Get(string? id) =>
        All.FirstOrDefault(p => p.Id == id) ?? All.First(p => p.Id == DefaultId);

    /// <summary>Recognizes keys with a distinctive prefix. Plain <c>sk-</c> keys are ambiguous.</summary>
    public static ProviderPreset? DetectFromKey(string key)
    {
        key = key.Trim();
        return All
            .Where(p => p.KeyHint is not null && key.StartsWith(p.KeyHint, StringComparison.Ordinal))
            .OrderByDescending(p => p.KeyHint!.Length)
            .FirstOrDefault();
    }
}
