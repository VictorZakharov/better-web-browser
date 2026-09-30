using ChromiumBaseline;
using System.Text.Json;

internal static class NativeWheelVerdictTests
{
    public static void Run()
    {
        Assert(NativeWheelVerdict.Parse(default, 1, 600) ==
            new NativeWheelVerdict(null, false, "probe_value_missing", null, null), "missing probe fabricated evidence");
        Check("null", 1, 600, "probe_value_missing", null, null);
        Check("[]", 1, 600, "event_sequence_missing", 0, null);
        Check("[{}]", 2, 600, "event_sequence_missing", 1, null);
        Check("[{}]", 0, 600, "event_sequence_missing", 1, null);
        Check("[null]", 1, 600, "event_verdict_invalid", 1, null);
        Check("[{}]", 1, 600, "event_verdict_invalid", 1, null);
        Check("[{\"deltaY\":\"600\"}]", 1, 600, "event_verdict_invalid", 1, null);
        Check("[{\"deltaY\":600.00000001}]", 1, 600, "event_delta_mismatch", 1, 600.00000001);
        Check("[{\"deltaY\":-600}]", 1, 600, "event_delta_mismatch", 1, -600);
        Check("[{\"deltaY\":600,\"defaultPrevented\":null,\"nested\":false}]", 1, 600,
            "default_prevented_pending", 1, 600);
        Check("[{\"deltaY\":600}]", 1, 600, "event_verdict_invalid", 1, 600);
        Check("[{\"deltaY\":600,\"defaultPrevented\":0,\"nested\":false}]", 1, 600,
            "event_verdict_invalid", 1, 600);
        Check("[{\"deltaY\":600,\"defaultPrevented\":false,\"nested\":null}]", 1, 600,
            "event_verdict_invalid", 1, 600);
        var observed = Parse("[{\"deltaY\":600,\"defaultPrevented\":false,\"nested\":false}]", 1, 600);
        Assert(observed == new NativeWheelVerdict(false, false, "observed", 1, 600), "valid ordinary verdict lost");
        var cancelled = Parse("[{\"deltaY\":600,\"defaultPrevented\":true,\"nested\":false}]", 1, 600);
        Assert(cancelled.DefaultPrevented == true && cancelled.Reason == "observed", "actual cancellation was lost");
        var nested = Parse("[{\"deltaY\":0},{\"deltaY\":-600,\"defaultPrevented\":false,\"nested\":true}]", 2, -600);
        Assert(nested == new NativeWheelVerdict(false, true, "observed", 2, -600), "sequence/reversed nested verdict lost");

        var samples = new NativeWheelSamples();
        samples.Navigated();
        samples.Frame(0, 0, null, null);
        var pending = samples.Begin(new(600, 300, 600), 0, 0)!;
        samples.Frame(600, 2, null, null);
        samples.Replied(pending, 3);
        samples.Finish(pending, Parse("[{\"deltaY\":600,\"defaultPrevented\":null,\"nested\":false}]", 1, 600));
        Assert(pending.Status == "listener_verdict_missing" && pending.ListenerVerdictReason == "default_prevented_pending" &&
            pending.ObservedListenerEvents == 1 && pending.ObservedEventDeltaY == 600 && pending.EnqueueToCdpReplyMs == 3 &&
            pending.DefaultPrevented is null && pending.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "pending flag retained uncertified latency or lost diagnostic evidence");
        var retired = samples.Begin(new(600, 300, 600), 0, 0)!;
        samples.Navigated();
        samples.Finish(retired, observed);
        Assert(retired.Status == "retired_document" && retired.ListenerVerdictReason is null &&
            retired.ObservedListenerEvents is null && retired.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "old document received replacement verdict evidence");
    }

    private static void Check(string json, int sequence, int delta, string reason, int? count, double? observedDelta)
    {
        var verdict = Parse(json, sequence, delta);
        Assert(verdict.Reason == reason && verdict.ObservedEvents == count && verdict.ObservedDelta == observedDelta &&
            verdict.DefaultPrevented is null, $"incorrect missing-verdict diagnosis for {json}");
    }

    private static NativeWheelVerdict Parse(string json, int sequence, int delta)
    {
        using var document = JsonDocument.Parse(json);
        return NativeWheelVerdict.Parse(document.RootElement, sequence, delta);
    }

    private static void Assert(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
