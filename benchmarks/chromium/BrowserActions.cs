using System.Text.Json;

namespace ChromiumBaseline;

internal static class BrowserActions
{
    public static async Task<int> RunAsync(
        CdpConnection cdp,
        Options options,
        TimeSpan timeout,
        int nextId)
    {
        if (options.ActivateLinkAfterReady is { } expectedUrl)
        {
            await Task.Delay(options.NavigationDelayMs);
            var serializedUrl = JsonSerializer.Serialize(expectedUrl);
            var point = await EvaluateAsync(cdp, nextId++, $$"""
                (() => {
                  const link = Array.from(document.querySelectorAll('a'))
                    .find(candidate => candidate.href === {{serializedUrl}});
                  if (!link) return null;
                  const rect = link.getBoundingClientRect();
                  return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
                })()
                """, timeout);
            if (point.ValueKind != JsonValueKind.Object)
            {
                throw new InvalidOperationException(
                    $"Chromium link was not presented: {expectedUrl}");
            }
            nextId = await DispatchClickAsync(
                cdp,
                point.GetProperty("x").GetDouble(),
                point.GetProperty("y").GetDouble(),
                timeout,
                nextId);
            await cdp.ReadUntilAsync(root =>
                root.TryGetProperty("method", out var method) &&
                method.GetString() == "Page.loadEventFired", timeout);
        }
        if (options.ClickAfterReady is { } click)
        {
            await Task.Delay(options.NavigationDelayMs);
            nextId = await DispatchClickAsync(cdp, click.X, click.Y, timeout, nextId);
        }
        return nextId;
    }

    private static async Task<JsonElement> EvaluateAsync(
        CdpConnection cdp,
        int id,
        string expression,
        TimeSpan timeout)
    {
        var response = await cdp.CallAsync(id, "Runtime.evaluate", new
        {
            expression,
            returnByValue = true,
            awaitPromise = true
        }, timeout);
        if (response.TryGetProperty("exceptionDetails", out var exception))
        {
            throw new InvalidOperationException($"Chromium evaluation failed: {exception}");
        }
        var remote = response.GetProperty("result");
        return remote.TryGetProperty("value", out var value) ? value.Clone() : default;
    }

    private static async Task<int> DispatchClickAsync(
        CdpConnection cdp,
        double x,
        double y,
        TimeSpan timeout,
        int nextId)
    {
        foreach (var type in new[] { "mousePressed", "mouseReleased" })
        {
            await cdp.CallAsync(nextId++, "Input.dispatchMouseEvent", new
            {
                type,
                x,
                y,
                button = "left",
                clickCount = 1
            }, timeout);
        }
        return nextId;
    }
}
