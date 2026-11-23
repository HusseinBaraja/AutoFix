using System.Diagnostics;
using AutoFix.Core.Settings;
using AutoFix.Core.Text;

namespace AutoFix.Core.Llm;

public sealed record CorrectionResult(string Text, bool Changed, long LatencyMs, bool Rejected);

/// <summary>Sends one correction request and returns a safe result.</summary>
public sealed class Corrector(LlmClient client)
{
    public async Task<CorrectionResult> CorrectAsync(
        ProviderConfig config,
        FixMode mode,
        string text,
        string context,
        IReadOnlyCollection<string> keep,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(config.Model) || string.IsNullOrWhiteSpace(config.BaseUrl))
            throw new LlmException(LlmErrorKind.NotConfigured, "No provider is configured.");

        var prompt = PromptBuilder.Build(text, context, mode, keep);
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(timeout);
        var watch = Stopwatch.StartNew();
        string response;
        try
        {
            response = await client.CompleteAsync(config, prompt, timeoutSource.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new LlmException(LlmErrorKind.Timeout, $"No answer within {timeout.TotalSeconds:0.#} s.");
        }
        long latency = watch.ElapsedMilliseconds;

        string? cleaned = ResponseGuard.Clean(text, response, mode == FixMode.Grammar);
        if (cleaned is null) return new CorrectionResult(text, false, latency, Rejected: true);
        return new CorrectionResult(cleaned, cleaned != text, latency, Rejected: false);
    }
}
