using ChromiumBaseline;

internal static class NativeWheelIntegrationTests
{
    public static async Task RunAsync(string chrome, string root)
    {
        var ordinary = await CaptureAsync(chrome, root, "ordinary", "", "", new WheelPoint(600, 300, 600), new WheelPoint(600, 300, -600));
        Assert(ordinary.Error is null, $"ordinary native wheel failed: {ordinary.Error}");
        Assert(ordinary.NativeWheel!.Samples.All(sample => sample.Status == "compositor_motion_observed" &&
            sample.ListenerVerdictReason == "observed" && sample.DefaultPrevented == false &&
            sample.ObservedEventDeltaY == sample.Input.Delta && !sample.ListenerVerdictDeadlineReached &&
            sample.EnqueueToFirstChangedCompositorFrameReceivedMs is not null), "trusted forward/reversed wheel was not observed");

        var cancelled = await CaptureAsync(chrome, root, "cancelled",
            "addEventListener('wheel', event => event.preventDefault(), {passive:false});", "", new WheelPoint(600, 300, 600));
        Assert(cancelled.Error is null && cancelled.NativeWheel!.Samples.Single().Status == "cancelled" &&
            cancelled.NativeWheel.Samples.Single().DefaultPrevented == true, "real page listener cancellation was not retained");
        AssertNoFrame(cancelled, "cancelled input gained frame latency");

        var noMotion = await CaptureAsync(chrome, root, "no-motion", "", "", new WheelPoint(600, 300, -600));
        Assert(noMotion.Error is null && noMotion.NativeWheel!.Samples.Single().Status == "no_viewport_motion_observed" &&
            noMotion.NativeWheel.Samples.Single().DefaultPrevented == false, "clamped wheel was conflated with cancellation/missing verdict");
        AssertNoFrame(noMotion, "clamped input gained frame latency");

        var nested = await CaptureAsync(chrome, root, "nested", "",
            "<div style='overflow:auto;width:300px;height:220px'><section style='height:2000px'>Nested owned scroller</section></div>",
            new WheelPoint(150, 100, 600));
        Assert(nested.Error is null && nested.NativeWheel!.Samples.Single().Status == "nested_frame_unattributed" &&
            nested.NativeWheel.Samples.Single().ObservedNestedScrollPort == true, "real nested target was not distinguished");
        AssertNoFrame(nested, "nested pixels were attributed using main-frame offsets");

        // A page's earlier passive capture listener can hide the event from our observer
        // without cancelling native scrolling. Missing observation must stay unknown.
        var missing = await CaptureAsync(chrome, root, "missing",
            "addEventListener('wheel', event => event.stopImmediatePropagation(), {capture:true,passive:true});",
            "", new WheelPoint(600, 300, 600));
        var missingSample = missing.NativeWheel!.Samples.Single();
        Assert(missing.Error is null && missingSample.Status == "listener_verdict_missing" &&
            missingSample.ListenerVerdictReason == "event_sequence_missing" && missingSample.ObservedListenerEvents == 0 &&
            missingSample.ListenerVerdictDeadlineReached && missingSample.DefaultPrevented is null,
            "hidden trusted observer fabricated a verdict or failed to reach its bounded deadline");
        AssertNoFrame(missing, "missing observer retained a frame latency");

        var retired = await CaptureAsync(chrome, root, "retired", """
            addEventListener('wheel', event => {
              event.stopImmediatePropagation();
              location.replace('about:blank');
            }, {capture:true,passive:true});
            """, "", new WheelPoint(600, 300, 600));
        Assert(retired.NativeWheel!.Samples.Single().Status == "retired_document" &&
            retired.NativeWheel.Samples.Single().DefaultPrevented is null, "replacement document certified an old wheel");
        AssertNoFrame(retired, "retired document retained a frame latency");
        Console.WriteLine("Six hidden trusted native-wheel fixture cases passed.");
    }

    private static async Task<BenchmarkResult> CaptureAsync(
        string chrome, string root, string name, string script, string prefix, params WheelPoint[] inputs)
    {
        var html = $$"""
            <!doctype html><html><head><style>body{margin:0}main{height:6000px;background:#dae7f8}</style></head><body>
            {{prefix}}<main>Owned ordinary native wheel diagnostic fixture.</main><script>{{script}}</script></body></html>
            """;
        var result = await ChromeRun.ExecuteAsync(new Options {
            Url = "data:text/html;charset=utf-8," + Uri.EscapeDataString(html),
            Output = Path.Combine(root, "native-wheel-" + name + ".json"), ChromePath = chrome,
            ViewportWidth = 1249, ViewportHeight = 548, DeviceScaleFactor = 1.25,
            InitialActionDelayMs = 1000, NavigationDelayMs = 1000,
            WheelAfterReady = inputs, SettleMs = 100, TimeoutMs = 15000
        });
        Assert(result.Headless && result.UnifiedHeadless && result.FreshProfile && result.CleanupError is null,
            $"fixture launch/cleanup contract failed: {name}");
        Assert(result.NativeWheel is { OmittedInputs: 0, UnattemptedInputs: 0 } &&
            result.NativeWheel.Samples.Count == inputs.Length, $"fixture silently omitted native inputs: {name}");
        return result;
    }

    private static void AssertNoFrame(BenchmarkResult result, string message) =>
        Assert(result.NativeWheel!.Samples.All(sample => sample.EnqueueToFirstChangedCompositorFrameReceivedMs is null &&
            sample.FirstChangedFrameScrollY is null && sample.FrameSwapTimestamp is null &&
            sample.FrameSwapMonotonicTimestamp is null), message);

    private static void Assert(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
