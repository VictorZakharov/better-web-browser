using System.Net.Http.Json;
using System.Text.Json;

namespace ChromiumBaseline;

internal static class ChromeProcessMemory
{
    internal static async Task<ProcessMemoryBreakdown> CollectAsync(int port,
        ProcessSample sample, TimeSpan timeout)
    {
        try
        {
            timeout = timeout < TimeSpan.FromSeconds(5) ? timeout : TimeSpan.FromSeconds(5);
            using var client = new HttpClient { Timeout = timeout };
            var version = await client.GetFromJsonAsync<JsonElement>($"http://127.0.0.1:{port}/json/version");
            var address = version.GetProperty("webSocketDebuggerUrl").GetString();
            if (!Uri.TryCreate(address, UriKind.Absolute, out var uri) || uri.Scheme != "ws" ||
                uri.Host != "127.0.0.1" || uri.Port != port ||
                !uri.AbsolutePath.StartsWith("/devtools/browser/", StringComparison.Ordinal))
                throw new InvalidOperationException("Chrome memory attribution requires the owned loopback browser target.");
            using var cdp = new CdpConnection();
            await cdp.ConnectAsync(uri, timeout);
            // SystemInfo is browser-target-only. Its IDs/types are labels, not
            // authority to open unowned processes. CPU comes from the same OS sample.
            // https://chromedevtools.github.io/devtools-protocol/tot/SystemInfo/#method-getProcessInfo
            var reply = await cdp.CallAsync(1, "SystemInfo.getProcessInfo", null, timeout);
            return ProcessMemoryBreakdown.From(sample, reply);
        }
        catch (Exception error) when (error is HttpRequestException or InvalidOperationException or
            JsonException or KeyNotFoundException or TimeoutException or OperationCanceledException or
            System.Net.WebSockets.WebSocketException)
        {
            // Preserve aggregate evidence; missing attribution is never zero renderer memory.
            return ProcessMemoryBreakdown.From(sample, null, "Chrome process-role attribution was unavailable.");
        }
    }
}
