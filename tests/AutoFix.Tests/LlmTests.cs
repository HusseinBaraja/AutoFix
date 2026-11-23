using System.Net;
using System.Text.Json.Nodes;
using AutoFix.Core.Llm;
using AutoFix.Core.Settings;

namespace AutoFix.Tests;

[TestClass]
public sealed class LlmClientTests
{
    private sealed class FakeHandler(Func<HttpRequestMessage, string, HttpResponseMessage> respond) : HttpMessageHandler
    {
        public List<(HttpRequestMessage Request, string Body)> Calls { get; } = [];

        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            string body = request.Content is null ? "" : await request.Content.ReadAsStringAsync(cancellationToken);
            Calls.Add((request, body));
            return respond(request, body);
        }
    }

    private static HttpResponseMessage Json(HttpStatusCode status, string json) =>
        new(status) { Content = new StringContent(json) };

    private static readonly Prompt SamplePrompt = new("sys", "<text>teh</text>", 64);

    [TestMethod]
    public async Task OpenAiRequestHasModelMessagesAndBearerKey()
    {
        var handler = new FakeHandler((_, _) =>
            Json(HttpStatusCode.OK, """{"choices":[{"message":{"content":"the"}}]}"""));
        using var client = new LlmClient(handler);

        string text = await client.CompleteAsync(
            new ProviderConfig(ApiKind.OpenAi, "https://api.deepseek.com/", "deepseek-chat", "sk-test"), SamplePrompt, default);

        Assert.AreEqual("the", text);
        var (request, body) = handler.Calls.Single();
        Assert.AreEqual("https://api.deepseek.com/chat/completions", request.RequestUri!.ToString());
        Assert.AreEqual("Bearer sk-test", request.Headers.Authorization!.ToString());
        var json = JsonNode.Parse(body)!;
        Assert.AreEqual("deepseek-chat", (string)json["model"]!);
        Assert.AreEqual("system", (string)json["messages"]![0]!["role"]!);
        Assert.AreEqual(0, (int)json["temperature"]!);
        Assert.AreEqual(64, (int)json["max_tokens"]!);
    }

    [TestMethod]
    public async Task AnthropicRequestUsesMessagesApi()
    {
        var handler = new FakeHandler((_, _) =>
            Json(HttpStatusCode.OK, """{"content":[{"type":"text","text":"the"}]}"""));
        using var client = new LlmClient(handler);

        string text = await client.CompleteAsync(
            new ProviderConfig(ApiKind.Anthropic, "https://api.anthropic.com", "claude-haiku-5-5", "sk-ant-x"), SamplePrompt, default);

        Assert.AreEqual("the", text);
        var (request, body) = handler.Calls.Single();
        Assert.AreEqual("https://api.anthropic.com/v1/messages", request.RequestUri!.ToString());
        Assert.AreEqual("sk-ant-x", request.Headers.GetValues("x-api-key").Single());
        Assert.AreEqual("sys", (string)JsonNode.Parse(body)!["system"]!);
    }

    [TestMethod]
    public async Task RetriesWithoutUnsupportedParameters()
    {
        var handler = new FakeHandler((_, body) => body.Contains("temperature")
            ? Json(HttpStatusCode.BadRequest, """{"error":{"message":"Unsupported parameter: 'temperature'"}}""")
            : Json(HttpStatusCode.OK, """{"choices":[{"message":{"content":"the"}}]}"""));
        using var client = new LlmClient(handler);
        var config = new ProviderConfig(ApiKind.OpenAi, "https://api.openai.com/v1", "gpt-5-nano", "k");

        Assert.AreEqual("the", await client.CompleteAsync(config, SamplePrompt, default));
        Assert.AreEqual("the", await client.CompleteAsync(config, SamplePrompt, default));

        Assert.AreEqual(3, handler.Calls.Count);
    }

    [TestMethod]
    public async Task MapsRejectedKey()
    {
        var handler = new FakeHandler((_, _) =>
            Json(HttpStatusCode.Unauthorized, """{"error":{"message":"Authentication Fails"}}"""));
        using var client = new LlmClient(handler);

        var error = await Assert.ThrowsExceptionAsync<LlmException>(() => client.CompleteAsync(
            new ProviderConfig(ApiKind.OpenAi, "https://x", "m", "bad"), SamplePrompt, default));

        Assert.AreEqual(LlmErrorKind.Auth, error.Kind);
        StringAssert.Contains(error.UserMessage, "API key");
    }

    [TestMethod]
    public async Task ListsModels()
    {
        var handler = new FakeHandler((_, _) =>
            Json(HttpStatusCode.OK, """{"data":[{"id":"b-model"},{"id":"a-model"}]}"""));
        using var client = new LlmClient(handler);

        var models = await client.ListModelsAsync(new ProviderConfig(ApiKind.OpenAi, "https://x/v1", "", "k"), default);

        CollectionAssert.AreEqual(new[] { "a-model", "b-model" }, models.ToArray());
        Assert.AreEqual("https://x/v1/models", handler.Calls[0].Request.RequestUri!.ToString());
    }

    [TestMethod]
    public async Task CorrectorTimesOut()
    {
        var handler = new SlowHandler();
        using var client = new LlmClient(handler);
        var corrector = new Corrector(client);

        var error = await Assert.ThrowsExceptionAsync<LlmException>(() => corrector.CorrectAsync(
            new ProviderConfig(ApiKind.OpenAi, "https://x", "m", "k"), FixMode.Spelling, "teh", "", [],
            TimeSpan.FromMilliseconds(50), default));

        Assert.AreEqual(LlmErrorKind.Timeout, error.Kind);
    }

    [TestMethod]
    public async Task CorrectorRejectsUnsafeAnswer()
    {
        var handler = new FakeHandler((_, _) =>
            Json(HttpStatusCode.OK, """{"choices":[{"message":{"content":"Paris is the capital of France, a country in Europe."}}]}"""));
        using var client = new LlmClient(handler);

        var result = await new Corrector(client).CorrectAsync(
            new ProviderConfig(ApiKind.OpenAi, "https://x", "m", "k"), FixMode.Spelling, "whats teh capital of france", "", [],
            TimeSpan.FromSeconds(5), default);

        Assert.IsTrue(result.Rejected);
        Assert.IsFalse(result.Changed);
    }

    private sealed class SlowHandler : HttpMessageHandler
    {
        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            await Task.Delay(5000, cancellationToken);
            return new HttpResponseMessage(HttpStatusCode.OK);
        }
    }
}

[TestClass]
public sealed class PromptAndProviderTests
{
    [TestMethod]
    public void PromptIncludesContextAndKeptWords()
    {
        var prompt = PromptBuilder.Build("teh", "Earlier text.", FixMode.Spelling, ["Madar"]);

        Assert.AreEqual(PromptBuilder.SpellingSystem, prompt.System);
        Assert.AreEqual("<context>Earlier text.</context>\n<keep>Madar</keep>\n<text>teh</text>", prompt.User);
    }

    [TestMethod]
    public void DetectsProviderFromKeyPrefix()
    {
        Assert.AreEqual("anthropic", ProviderCatalog.DetectFromKey("sk-ant-api03-abc")?.Id);
        Assert.AreEqual("openrouter", ProviderCatalog.DetectFromKey("sk-or-v1-abc")?.Id);
        Assert.AreEqual("groq", ProviderCatalog.DetectFromKey("gsk_abc")?.Id);
        Assert.AreEqual("gemini", ProviderCatalog.DetectFromKey("AIzaSyabc")?.Id);
        Assert.IsNull(ProviderCatalog.DetectFromKey("sk-1234"));
    }
}

[TestClass]
public sealed class SettingsTests
{
    [TestMethod]
    public void RoundTripsSettings()
    {
        string path = Path.Combine(Path.GetTempPath(), Guid.NewGuid() + ".json");
        try
        {
            var store = new SettingsStore(path);
            var settings = new AppSettings { Provider = "groq", Mode = FixMode.Grammar, Dictionary = ["Madar"] };
            settings.ProviderSettingsFor("groq").Model = "llama-3.1-8b-instant";

            store.Save(settings);
            var loaded = store.Load();

            Assert.AreEqual("groq", loaded.Provider);
            Assert.AreEqual(FixMode.Grammar, loaded.Mode);
            Assert.AreEqual("llama-3.1-8b-instant", loaded.ProviderConfig("k").Model);
            CollectionAssert.AreEqual(new[] { "Madar" }, loaded.Dictionary);
            StringAssert.Contains(File.ReadAllText(path), "\"grammar\"");
        }
        finally
        {
            File.Delete(path);
        }
    }

    [TestMethod]
    public void DamagedFileGivesDefaultsAndKeepsCopy()
    {
        string path = Path.Combine(Path.GetTempPath(), Guid.NewGuid() + ".json");
        try
        {
            File.WriteAllText(path, "{ not json");

            var loaded = new SettingsStore(path).Load();

            Assert.AreEqual(ProviderCatalog.DefaultId, loaded.Provider);
            Assert.IsTrue(File.Exists(path + ".bad"));
        }
        finally
        {
            File.Delete(path);
            File.Delete(path + ".bad");
        }
    }

    [TestMethod]
    public void BlockedAppsIgnoreCase() => Assert.IsTrue(new AppSettings().IsBlocked("code.EXE"));

    [TestMethod]
    public void ParsesAndFormatsShortcuts()
    {
        var chord = KeyChord.TryParse("ctrl + alt + space");

        Assert.IsNotNull(chord);
        Assert.AreEqual("Ctrl+Alt+Space", chord.ToString());
        Assert.IsNull(KeyChord.TryParse("Shift+A"));
        Assert.IsNull(KeyChord.TryParse("Ctrl+Nope"));
        Assert.AreEqual("F8", KeyChord.TryParse("F8")?.ToString());
    }
}
