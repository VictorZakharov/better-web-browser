using System.Diagnostics;
using System.Text.Json;

namespace ChromiumBaseline;

internal sealed class NativeWheelTrace : IDisposable
{
    private readonly CdpConnection cdp;
    private readonly Stopwatch stopwatch;
    private readonly object gate = new();
    private readonly IDisposable subscription;
    private readonly NativeWheelSamples samples = new();
    public NativeWheelResult Result => samples.Result;
    private TaskCompletionSource frameChanged = NewCompletion();

    public NativeWheelTrace(CdpConnection cdp, Stopwatch stopwatch)
    {
        this.cdp = cdp;
        this.stopwatch = stopwatch;
        subscription = cdp.Observe(root => {
            if (!root.TryGetProperty("method", out var method)) return;
            if (method.GetString() is not ("Page.frameNavigated" or "Page.screencastFrame")) return;
            var parameters = root.GetProperty("params");
            lock (gate)
            {
                if (method.GetString() == "Page.frameNavigated" && !parameters.GetProperty("frame").TryGetProperty("parentId", out _))
                    samples.Navigated();
                if (method.GetString() != "Page.screencastFrame") return;
                var metadata = parameters.GetProperty("metadata");
                samples.Frame(metadata.GetProperty("scrollOffsetY").GetDouble(), stopwatch.Elapsed.TotalMilliseconds,
                    Number(metadata, "timestamp"), Number(metadata, "monotonicTimestamp"));
                if (samples.Result.Samples.LastOrDefault()?.EnqueueToFirstChangedCompositorFrameReceivedMs is not null) frameChanged.TrySetResult();
            }
        });
    }

    public async Task<(NativeWheelResult Result, int NextId)> RunAsync(Options options, TimeSpan timeout, int nextId, double pageReadyMs)
    {
        samples.Result.RequestedInputs = options.WheelAfterReady.Count;
        // An isolated, passive observer cannot cancel input or turn a compositor-eligible
        // wheel into a blocking page listener. Its delayed verdict is diagnostic only.
        var tree = await cdp.CallAsync(nextId++, "Page.getFrameTree", null, timeout);
        var world = await cdp.CallAsync(nextId++, "Page.createIsolatedWorld", new {
            frameId = tree.GetProperty("frameTree").GetProperty("frame").GetProperty("id").GetString(),
            worldName = "breeze-native-wheel-diagnostics"
        }, timeout);
        var context = world.GetProperty("executionContextId").GetInt32();
        await EvaluateAsync(nextId++, context, ProbeScript, timeout);
        var anchor = pageReadyMs;
        var firstDelay = options.NextActionDelayMs();
        for (var index = 0; index < options.WheelAfterReady.Count; index++)
        {
            var scheduled = anchor + firstDelay + index * (double)options.NavigationDelayMs;
            var delay = scheduled - stopwatch.Elapsed.TotalMilliseconds;
            if (delay > 0) await Task.Delay(TimeSpan.FromMilliseconds(delay));
            NativeWheelSample? sample;
            Task frame;
            lock (gate)
            {
                frameChanged = NewCompletion();
                sample = samples.Begin(options.WheelAfterReady[index], scheduled, stopwatch.Elapsed.TotalMilliseconds);
                frame = frameChanged.Task;
            }
            if (sample is null) continue;
            try
            {
                await cdp.CallAsync(nextId++, "Input.dispatchMouseEvent", new {
                    type = "mouseWheel", x = sample.Input.X, y = sample.Input.Y,
                    deltaX = 0, deltaY = sample.Input.Delta
                }, timeout);
                lock (gate) samples.Replied(sample, stopwatch.Elapsed.TotalMilliseconds);
                await Task.WhenAny(frame, Task.Delay(Math.Clamp(options.NavigationDelayMs * 3 / 4, 100, 1000)));
                var verdicts = await EvaluateAsync(nextId++, context, "globalThis.__nativeWheelTrace", timeout);
                bool? cancelled = null;
                var nested = false;
                if (verdicts.ValueKind == JsonValueKind.Array && verdicts.GetArrayLength() >= sample.Sequence)
                {
                    var verdict = verdicts[sample.Sequence - 1];
                    if (verdict.GetProperty("deltaY").GetDouble() == sample.Input.Delta)
                    {
                        var prevented = verdict.GetProperty("defaultPrevented");
                        cancelled = prevented.ValueKind is JsonValueKind.True or JsonValueKind.False ? prevented.GetBoolean() : null;
                        nested = verdict.GetProperty("nested").GetBoolean();
                    }
                }
                lock (gate) samples.Finish(sample, cancelled, nested);
            }
            catch
            {
                lock (gate) samples.Failed(sample);
                throw;
            }
        }
        return (samples.Result, nextId);
    }

    private async Task<JsonElement> EvaluateAsync(int id, int context, string expression, TimeSpan timeout)
    {
        var response = await cdp.CallAsync(id, "Runtime.evaluate", new { expression, contextId = context, returnByValue = true }, timeout);
        if (response.TryGetProperty("exceptionDetails", out var error)) throw new InvalidOperationException($"Wheel diagnostic evaluation failed: {error}");
        return response.GetProperty("result").TryGetProperty("value", out var value) ? value.Clone() : default;
    }

    private static TaskCompletionSource NewCompletion() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static double? Number(JsonElement value, string key) => value.TryGetProperty(key, out var item) && item.ValueKind == JsonValueKind.Number && item.TryGetDouble(out var number) && double.IsFinite(number) ? number : null;
    public void Dispose() => subscription.Dispose();

    private const string ProbeScript = """
        globalThis.__nativeWheelTrace = [];
        addEventListener('wheel', event => {
          if (!event.isTrusted || __nativeWheelTrace.length >= 128) return;
          const nested = event.composedPath().slice(0, 256).some(node => node instanceof Element &&
            node !== document.scrollingElement &&
            (node.scrollHeight > node.clientHeight || node.scrollWidth > node.clientWidth) &&
            ['auto','scroll'].includes(getComputedStyle(node).overflowY));
          const verdict = {deltaY:event.deltaY, defaultPrevented:null, nested};
          __nativeWheelTrace.push(verdict);
          setTimeout(() => { verdict.defaultPrevented = event.defaultPrevented; }, 0);
        }, {capture:true, passive:true});
        0
        """;
}
