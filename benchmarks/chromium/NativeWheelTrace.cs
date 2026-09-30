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
        // wheel into a blocking page listener. Its post-dispatch verdict is diagnostic only.
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
                var acknowledgedMs = stopwatch.Elapsed.TotalMilliseconds;
                var observation = new NativeWheelObservation(acknowledgedMs, options.NavigationDelayMs);
                lock (gate)
                {
                    samples.Replied(sample, acknowledgedMs);
                    sample.ObservationDeadlineMs = observation.DeadlineMs;
                }
                var acquired = await AcquireVerdictAsync(sample, observation, context, timeout, nextId);
                nextId = acquired.NextId;
                var verdict = acquired.Verdict;
                if (Retired(sample)) break;
                if (verdict.DefaultPrevented == false && !verdict.Nested && !frame.IsCompleted)
                {
                    var remaining = observation.RemainingMs(stopwatch.Elapsed.TotalMilliseconds);
                    if (remaining > 0) await Task.WhenAny(frame, Task.Delay(TimeSpan.FromMilliseconds(remaining)));
                }
                lock (gate) samples.Finish(sample, verdict);
                if (Retired(sample)) break;
            }
            catch
            {
                lock (gate) samples.Failed(sample);
                throw;
            }
        }
        return (samples.Result, nextId);
    }

    private async Task<(NativeWheelVerdict Verdict, int NextId)> AcquireVerdictAsync(
        NativeWheelSample sample, NativeWheelObservation observation, int context, TimeSpan timeout, int nextId)
    {
        var verdict = NativeWheelVerdict.Parse(default, sample.Sequence, sample.Input.Delta);
        while (!Retired(sample) && observation.RemainingMs(stopwatch.Elapsed.TotalMilliseconds) > 0)
        {
            var remaining = observation.RemainingMs(stopwatch.Elapsed.TotalMilliseconds);
            if (remaining <= 0) break;
            try
            {
                var snapshot = await EvaluateAsync(nextId++, context, SnapshotScript,
                    TimeSpan.FromMilliseconds(Math.Min(remaining, timeout.TotalMilliseconds)));
                verdict = NativeWheelVerdict.Parse(snapshot, sample.Sequence, sample.Input.Delta);
            }
            catch (TimeoutException) { break; }
            catch (InvalidOperationException) when (Retired(sample)) { return (verdict, nextId); }
            var now = stopwatch.Elapsed.TotalMilliseconds;
            if (observation.Accepts(verdict, now)) return (verdict, nextId);
            var delay = observation.PollDelayMs(now);
            if (delay > 0) await Task.Delay(TimeSpan.FromMilliseconds(delay));
        }
        return (observation.Expired(verdict), nextId);
    }

    private bool Retired(NativeWheelSample sample)
    {
        lock (gate) return sample.DocumentEpoch != samples.DocumentEpoch || sample.Status == "retired_document";
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
          __nativeWheelTrace.push({event, nested});
        }, {capture:true, passive:true});
        0
        """;

    // DOM dispatch resets eventPhase to NONE after listener propagation; its canceled
    // flag survives dispatch. Read retained events in this isolated context without
    // depending on a separately scheduled timer: https://dom.spec.whatwg.org/#concept-event-dispatch
    private const string SnapshotScript = """
        Array.isArray(globalThis.__nativeWheelTrace) ? globalThis.__nativeWheelTrace.map(({event,nested}) => ({
          deltaY:event.deltaY,
          defaultPrevented:event.eventPhase === Event.NONE ? event.defaultPrevented : null,
          nested
        })) : null
        """;
}
