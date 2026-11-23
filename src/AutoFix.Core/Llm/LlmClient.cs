using System.Collections.Concurrent;
using System.Net;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AutoFix.Core.Llm;

public sealed record ProviderConfig(ApiKind Kind, string BaseUrl, string Model, string? ApiKey);

public enum LlmErrorKind
{
    Auth,
    NotFound,
    RateLimit,
    BadRequest,
    Server,
    Timeout,
    Network,
    BadResponse,
    NotConfigured,
}

public sealed class LlmException(LlmErrorKind kind, string message, int? status = null) : Exception(message)
{
    public LlmErrorKind Kind { get; } = kind;
    public int? Status { get; } = status;

    /// <summary>A short message the user can act on.</summary>
    public string UserMessage => Kind switch
    {
        LlmErrorKind.Auth => "The API key was rejected. Check the key in AutoFix settings.",
        LlmErrorKind.NotFound => "The provider does not know this model. Choose another model in settings.",
        LlmErrorKind.RateLimit => "The provider is rate limiting requests or the account is out of credit.",
        LlmErrorKind.Timeout => "The provider took too long to answer.",
        LlmErrorKind.Network => "AutoFix cannot reach the provider. Check your internet connection.",
        LlmErrorKind.NotConfigured => "Add an API key in AutoFix settings.",
        LlmErrorKind.Server => "The provider had a server error. Try again shortly.",
        _ => Message,
    };
}

/// <summary>
/// Minimal client for OpenAI-compatible and Anthropic chat APIs. One pooled HTTP
/// connection is kept warm so a correction costs one round trip.
/// </summary>
public sealed class LlmClient : IDisposable
{
    private readonly HttpClient _http;
    // Some models reject parameters (for example temperature on reasoning models); remember per model.
    private readonly ConcurrentDictionary<string, Quirks> _quirks = new();

    [Flags]
    private enum Quirks
    {
        None = 0,
        NoTemperature = 1,
        MaxCompletionTokens = 2,
    }

    public LlmClient(HttpMessageHandler? handler = null)
    {
        handler ??= new SocketsHttpHandler
        {
            PooledConnectionIdleTimeout = TimeSpan.FromMinutes(10),
            PooledConnectionLifetime = TimeSpan.FromMinutes(30),
            ConnectTimeout = TimeSpan.FromSeconds(8),
            AutomaticDecompression = DecompressionMethods.All,
            EnableMultipleHttp2Connections = true,
        };
        _http = new HttpClient(handler)
        {
            Timeout = Timeout.InfiniteTimeSpan,
            DefaultRequestVersion = HttpVersion.Version20,
            DefaultVersionPolicy = HttpVersionPolicy.RequestVersionOrLower,
        };
        _http.DefaultRequestHeaders.UserAgent.ParseAdd("AutoFix/2.0");
    }

    public async Task<string> CompleteAsync(ProviderConfig config, Prompt prompt, CancellationToken cancellationToken)
    {
        string quirkKey = config.BaseUrl + "|" + config.Model;
        for (int attempt = 0; ; attempt++)
        {
            var quirks = _quirks.GetValueOrDefault(quirkKey);
            using var request = config.Kind == ApiKind.Anthropic
                ? BuildAnthropic(config, prompt)
                : BuildOpenAi(config, prompt, quirks);
            try
            {
                return await SendAsync(request, config.Kind, cancellationToken).ConfigureAwait(false);
            }
            catch (LlmException error) when (attempt == 0 && error.Kind == LlmErrorKind.BadRequest
                && config.Kind == ApiKind.OpenAi && Adjust(error.Message, quirks) is { } adjusted)
            {
                _quirks[quirkKey] = adjusted;
            }
        }
    }

    private static Quirks? Adjust(string message, Quirks quirks)
    {
        var updated = quirks;
        if (message.Contains("temperature", StringComparison.OrdinalIgnoreCase)) updated |= Quirks.NoTemperature;
        if (message.Contains("max_tokens", StringComparison.OrdinalIgnoreCase)
            || message.Contains("max_completion_tokens", StringComparison.OrdinalIgnoreCase))
            updated |= Quirks.MaxCompletionTokens;
        return updated == quirks ? null : updated;
    }

    public async Task<IReadOnlyList<string>> ListModelsAsync(ProviderConfig config, CancellationToken cancellationToken)
    {
        string url = config.Kind == ApiKind.Anthropic ? AnthropicUrl(config.BaseUrl, "models") : Url(config.BaseUrl, "models");
        using var request = new HttpRequestMessage(HttpMethod.Get, url);
        AddAuth(request, config);
        using var response = await SendRawAsync(request, cancellationToken).ConfigureAwait(false);
        string body = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode) throw Error(response.StatusCode, body);
        try
        {
            var data = JsonNode.Parse(body)?["data"]?.AsArray() ?? [];
            return data.Select(m => m?["id"]?.GetValue<string>()).OfType<string>().Order(StringComparer.OrdinalIgnoreCase).ToList();
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException)
        {
            throw new LlmException(LlmErrorKind.BadResponse, "The provider returned an unreadable model list.");
        }
    }

    /// <summary>Opens the TLS connection ahead of time so the next correction skips the handshake.</summary>
    public async Task WarmUpAsync(ProviderConfig config, CancellationToken cancellationToken)
    {
        if (!Uri.TryCreate(config.BaseUrl, UriKind.Absolute, out var uri)) return;
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Head, new Uri(uri.GetLeftPart(UriPartial.Authority) + "/"));
            using var response = await _http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or OperationCanceledException)
        {
            // Warm-up is best effort.
        }
    }

    private static HttpRequestMessage BuildOpenAi(ProviderConfig config, Prompt prompt, Quirks quirks)
    {
        var body = new JsonObject
        {
            ["model"] = config.Model,
            ["messages"] = new JsonArray
            {
                new JsonObject { ["role"] = "system", ["content"] = prompt.System },
                new JsonObject { ["role"] = "user", ["content"] = prompt.User },
            },
            ["stream"] = false,
        };
        if (!quirks.HasFlag(Quirks.NoTemperature)) body["temperature"] = 0;
        body[quirks.HasFlag(Quirks.MaxCompletionTokens) ? "max_completion_tokens" : "max_tokens"] = prompt.MaxTokens;

        var request = new HttpRequestMessage(HttpMethod.Post, Url(config.BaseUrl, "chat/completions"))
        {
            Content = Json(body),
        };
        AddAuth(request, config);
        return request;
    }

    private static HttpRequestMessage BuildAnthropic(ProviderConfig config, Prompt prompt)
    {
        var body = new JsonObject
        {
            ["model"] = config.Model,
            ["max_tokens"] = prompt.MaxTokens,
            ["temperature"] = 0,
            ["system"] = prompt.System,
            ["messages"] = new JsonArray
            {
                new JsonObject { ["role"] = "user", ["content"] = prompt.User },
            },
        };
        var request = new HttpRequestMessage(HttpMethod.Post, AnthropicUrl(config.BaseUrl, "messages")) { Content = Json(body) };
        AddAuth(request, config);
        return request;
    }

    private static void AddAuth(HttpRequestMessage request, ProviderConfig config)
    {
        if (string.IsNullOrEmpty(config.ApiKey)) return;
        if (config.Kind == ApiKind.Anthropic)
        {
            request.Headers.Add("x-api-key", config.ApiKey);
            request.Headers.Add("anthropic-version", "2023-06-01");
        }
        else
        {
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", config.ApiKey);
        }
    }

    private async Task<string> SendAsync(HttpRequestMessage request, ApiKind kind, CancellationToken cancellationToken)
    {
        using var response = await SendRawAsync(request, cancellationToken).ConfigureAwait(false);
        string body;
        try
        {
            body = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (HttpRequestException)
        {
            throw new LlmException(LlmErrorKind.Network, "The connection closed while reading the answer.");
        }
        if (!response.IsSuccessStatusCode) throw Error(response.StatusCode, body);
        return ParseText(body, kind);
    }

    private async Task<HttpResponseMessage> SendRawAsync(HttpRequestMessage request, CancellationToken cancellationToken)
    {
        try
        {
            return await _http.SendAsync(request, HttpCompletionOption.ResponseContentRead, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex) when (ex is HttpRequestException or OperationCanceledException)
        {
            throw new LlmException(LlmErrorKind.Network, ex.Message);
        }
    }

    internal static string ParseText(string body, ApiKind kind)
    {
        try
        {
            var root = JsonNode.Parse(body);
            string? text;
            if (kind == ApiKind.Anthropic)
            {
                var parts = root?["content"]?.AsArray() ?? [];
                text = string.Concat(parts.Where(p => p?["type"]?.GetValue<string>() == "text")
                    .Select(p => p?["text"]?.GetValue<string>()));
            }
            else
            {
                var content = root?["choices"]?[0]?["message"]?["content"];
                text = content is JsonArray array
                    ? string.Concat(array.Select(p => p?["text"]?.GetValue<string>()))
                    : content?.GetValue<string>();
            }
            return text ?? throw new LlmException(LlmErrorKind.BadResponse, "The provider returned no text.");
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException or FormatException)
        {
            throw new LlmException(LlmErrorKind.BadResponse, "The provider returned an unreadable answer.");
        }
    }

    internal static LlmException Error(HttpStatusCode status, string body)
    {
        string detail = ErrorDetail(body) ?? $"HTTP {(int)status}";
        var kind = (int)status switch
        {
            401 or 403 => LlmErrorKind.Auth,
            402 or 429 => LlmErrorKind.RateLimit,
            404 => LlmErrorKind.NotFound,
            >= 500 => LlmErrorKind.Server,
            _ => LlmErrorKind.BadRequest,
        };
        // Some providers report an unknown model as 400.
        if (kind == LlmErrorKind.BadRequest && detail.Contains("model", StringComparison.OrdinalIgnoreCase)
            && (detail.Contains("not exist", StringComparison.OrdinalIgnoreCase) || detail.Contains("not found", StringComparison.OrdinalIgnoreCase)))
            kind = LlmErrorKind.NotFound;
        return new LlmException(kind, detail, (int)status);
    }

    private static string? ErrorDetail(string body)
    {
        try
        {
            var root = JsonNode.Parse(body);
            var error = root is JsonArray list ? list[0]?["error"] : root?["error"];
            return error is JsonValue value ? value.GetValue<string>() : error?["message"]?.GetValue<string>();
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException or ArgumentOutOfRangeException)
        {
            return body.Length is > 0 and < 300 ? body : null;
        }
    }

    private static StringContent Json(JsonNode body) =>
        new(body.ToJsonString(), Encoding.UTF8, "application/json");

    internal static string Url(string baseUrl, string path) => baseUrl.TrimEnd('/') + "/" + path;

    internal static string AnthropicUrl(string baseUrl, string path)
    {
        string root = baseUrl.TrimEnd('/');
        return root.EndsWith("/v1", StringComparison.Ordinal) ? root + "/" + path : root + "/v1/" + path;
    }

    public void Dispose() => _http.Dispose();
}
