using System.Net.WebSockets;
using System.Text.Json;

namespace ChromiumBaseline;

internal sealed class CdpConnection : IDisposable
{
    private readonly ClientWebSocket socket = new();

    public async Task ConnectAsync(Uri uri, TimeSpan timeout)
    {
        using var cancellation = new CancellationTokenSource(timeout);
        await socket.ConnectAsync(uri, cancellation.Token);
    }

    public async Task SendAsync(object message)
    {
        await socket.SendAsync(
            JsonSerializer.SerializeToUtf8Bytes(message),
            WebSocketMessageType.Text,
            true,
            CancellationToken.None);
    }

    public async Task<JsonElement> CallAsync(int id, string method, object? parameters, TimeSpan timeout)
    {
        var message = new Dictionary<string, object?> { ["id"] = id, ["method"] = method };
        if (parameters is not null)
        {
            message["params"] = parameters;
        }
        await SendAsync(message);
        JsonElement response = default;
        await ReadUntilAsync(root =>
        {
            if (!root.TryGetProperty("id", out var responseId) || responseId.GetInt32() != id)
            {
                return false;
            }
            response = root.Clone();
            return true;
        }, timeout);
        if (response.TryGetProperty("error", out var error))
        {
            throw new InvalidOperationException($"CDP {method} failed: {error}");
        }
        return response.GetProperty("result").Clone();
    }

    public async Task ReadUntilAsync(Func<JsonElement, bool> predicate, TimeSpan timeout)
    {
        using var cancellation = new CancellationTokenSource(timeout);
        while (true)
        {
            using var document = await ReceiveAsync(cancellation.Token);
            if (predicate(document.RootElement))
            {
                return;
            }
        }
    }

    private async Task<JsonDocument> ReceiveAsync(CancellationToken cancellation)
    {
        using var stream = new MemoryStream();
        var buffer = new byte[32 * 1024];
        WebSocketReceiveResult result;
        do
        {
            result = await socket.ReceiveAsync(buffer, cancellation);
            if (result.MessageType == WebSocketMessageType.Close)
            {
                throw new WebSocketException("Chromium closed the DevTools connection.");
            }
            stream.Write(buffer, 0, result.Count);
        } while (!result.EndOfMessage);
        stream.Position = 0;
        return await JsonDocument.ParseAsync(stream, cancellationToken: cancellation);
    }

    public void Dispose() => socket.Dispose();
}
