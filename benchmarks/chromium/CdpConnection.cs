using System.Net.WebSockets;
using System.Text.Json;
using System.Collections.Concurrent;

namespace ChromiumBaseline;

internal sealed class CdpConnection : IDisposable
{
    private readonly ClientWebSocket socket = new();
    private readonly SemaphoreSlim sendGate = new(1);
    private readonly CancellationTokenSource lifetime = new();
    private readonly ConcurrentDictionary<int, TaskCompletionSource<JsonElement>> calls = new();
    private readonly ConcurrentDictionary<TaskCompletionSource, Func<JsonElement, bool>> listeners = new();
    private Task? receiver;

    public async Task ConnectAsync(Uri uri, TimeSpan timeout)
    {
        using var cancellation = new CancellationTokenSource(timeout);
        await socket.ConnectAsync(uri, cancellation.Token);
        receiver = ReceiveLoopAsync();
    }

    public async Task SendAsync(object message)
    {
        await sendGate.WaitAsync(lifetime.Token);
        try
        {
            await socket.SendAsync(JsonSerializer.SerializeToUtf8Bytes(message),
                WebSocketMessageType.Text, true, lifetime.Token);
        }
        finally { sendGate.Release(); }
    }

    public async Task<JsonElement> CallAsync(int id, string method, object? parameters, TimeSpan timeout)
    {
        var message = new Dictionary<string, object?> { ["id"] = id, ["method"] = method };
        if (parameters is not null)
        {
            message["params"] = parameters;
        }
        var completion = new TaskCompletionSource<JsonElement>(TaskCreationOptions.RunContinuationsAsynchronously);
        if (!calls.TryAdd(id, completion)) throw new InvalidOperationException($"Duplicate CDP id {id}.");
        JsonElement response;
        try
        {
            await SendAsync(message);
            response = await completion.Task.WaitAsync(timeout, lifetime.Token);
        }
        finally { calls.TryRemove(id, out _); }
        if (response.TryGetProperty("error", out var error))
        {
            throw new InvalidOperationException($"CDP {method} failed: {error}");
        }
        return response.GetProperty("result").Clone();
    }

    public async Task ReadUntilAsync(Func<JsonElement, bool> predicate, TimeSpan timeout)
    {
        // Register before triggering the action: a fast response must not race its observer.
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        listeners.TryAdd(completion, predicate);
        try { await completion.Task.WaitAsync(timeout, lifetime.Token); }
        finally { listeners.TryRemove(completion, out _); }
    }

    private async Task ReceiveLoopAsync()
    {
        try
        {
            while (!lifetime.IsCancellationRequested)
            {
                using var document = await ReceiveAsync(lifetime.Token);
                var root = document.RootElement;
                if (root.TryGetProperty("id", out var id) && calls.TryGetValue(id.GetInt32(), out var call))
                    call.TrySetResult(root.Clone());
                foreach (var (completion, predicate) in listeners)
                {
                    try { if (predicate(root)) completion.TrySetResult(); }
                    catch (Exception error) { completion.TrySetException(error); }
                }
            }
        }
        catch (Exception error)
        {
            foreach (var call in calls.Values) call.TrySetException(error);
            foreach (var listener in listeners.Keys) listener.TrySetException(error);
            lifetime.Cancel();
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
        // DOM.getDocument is the one CDP response that preserves tree nesting;
        // ordinary production pages can exceed System.Text.Json's default depth.
        return await JsonDocument.ParseAsync(
            stream,
            new JsonDocumentOptions { MaxDepth = 512 },
            cancellation);
    }

    public void Dispose()
    {
        lifetime.Cancel();
        socket.Dispose();
        receiver?.GetAwaiter().GetResult();
        lifetime.Dispose();
    }

    public IDisposable Observe(Action<JsonElement> observer)
    {
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        listeners.TryAdd(completion, root => { observer(root); return false; });
        return new Subscription(() => listeners.TryRemove(completion, out _));
    }

    private sealed class Subscription(Action remove) : IDisposable
    {
        public void Dispose() => remove();
    }
}
