using ChromiumBaseline;
using System.Diagnostics;

internal static class NativeWheelTests
{
    public static void Run()
    {
        NativeWheelVerdictTests.Run();
        NativeWheelObservationTests.Run();
        var samples = new NativeWheelSamples();
        samples.Navigated();
        samples.Frame(0, 0, 100, 200);
        var down = samples.Begin(new(600, 300, 600), 1000, 1002)!;
        samples.Frame(20, 1018, 101, 201);
        samples.Replied(down, 1020);
        samples.Finish(down, false, false);
        Assert(down.EnqueueToCdpReplyMs == 18 && down.EnqueueToFirstChangedCompositorFrameReceivedMs == 16,
            "native CDP reply/frame endpoints were conflated");
        Assert(down.FrameSwapTimestamp == 101 && down.FrameSwapMonotonicTimestamp == 201,
            "raw compositor clock evidence was lost");
        var reversed = samples.Begin(new(600, 300, -600), 2000, 2000)!;
        samples.Frame(25, 2002, null, null); // An old positive-direction animation cannot satisfy this input.
        Assert(reversed.EnqueueToFirstChangedCompositorFrameReceivedMs is null, "opposite motion credited a reversed wheel");
        samples.Frame(0, 2020, null, null);
        samples.Replied(reversed, 2021);
        samples.Finish(reversed, false, false);
        Assert(reversed.Status == "compositor_motion_observed", "reversed wheel motion was lost");

        var cancelled = samples.Begin(new(300, 50, 126), 3000, 3000)!;
        samples.Frame(30, 3004, null, null);
        samples.Replied(cancelled, 3010);
        samples.Finish(cancelled, true, false);
        Assert(cancelled.Status == "cancelled" && cancelled.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "cancelled input retained an unrelated motion latency");
        var nested = samples.Begin(new(50, 50, 75), 4000, 4000)!;
        samples.Replied(nested, 4010);
        samples.Finish(nested, false, true);
        Assert(nested.Status == "nested_frame_unattributed" && nested.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "main-frame scroll metadata falsely measured nested pixels");
        var zero = samples.Begin(new(600, 300, 0), 5000, 5000)!;
        samples.Frame(40, 5002, null, null);
        samples.Replied(zero, 5003);
        samples.Finish(zero, false, false);
        Assert(zero.Status == "no_viewport_motion_observed" && zero.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "zero wheel fabricated a paint latency");
        var stale = samples.Begin(new(600, 300, 600), 6000, 6000)!;
        samples.Navigated();
        samples.Frame(0, 6002, null, null);
        samples.Replied(stale, 6003);
        samples.Finish(stale, false, false);
        Assert(stale.Status == "retired_document" && stale.EnqueueToCdpReplyMs is null && stale.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "replacement document acknowledged a stale input");
        samples.Frame(0, 6100, null, null);
        var missing = samples.Begin(new(600, 300, 600), 7000, 7000)!;
        samples.Frame(20, 7002, null, null);
        samples.Replied(missing, 7003);
        samples.Finish(missing, null, false);
        Assert(missing.Status == "listener_verdict_missing" && missing.EnqueueToCdpReplyMs == 3 && missing.DefaultPrevented is null && missing.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "missing listener verdict was conflated with cancellation or paint latency");
        var superseded = samples.Begin(new(600, 300, 600), 7100, 7100)!;
        samples.Frame(40, 7101, null, null);
        var replacing = samples.Begin(new(600, 300, -600), 7102, 7102)!;
        Assert(superseded.Status == "superseded" && superseded.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "superseded input retained a claimed frame latency");
        samples.Frame(20, 7103, null, null);
        samples.Navigated();
        Assert(replacing.Status == "retired_document" && replacing.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "document retirement retained an unfinished frame attribution");
        var unacknowledged = samples.Begin(new(600, 300, 600), 7200, 7200)!;
        samples.Frame(50, 7201, null, null);
        samples.Finish(unacknowledged, false, false);
        Assert(unacknowledged.Status == "unacknowledged" && unacknowledged.EnqueueToCdpReplyMs is null,
            "unacknowledged input was silently classified as no motion");
        samples.Failed(unacknowledged);
        Assert(unacknowledged.Status == "dispatch_failed" && unacknowledged.EnqueueToFirstChangedCompositorFrameReceivedMs is null,
            "dispatch failure fabricated motion");
        for (var i = samples.Result.Samples.Count; i < 130; i++) samples.Begin(new(600, 300, 600), i, i);
        Assert(samples.Result.Samples.Count == 128 && samples.Result.OmittedInputs == 2, "sample cap omitted diagnostics silently");

        Assert(WheelPoint.Parse("600, 300, -600") == new WheelPoint(600, 300, -600), "CSS wheel tuple parsing failed");
        foreach (var invalid in new[] { "-1,0,1", "0,4321,1", "0,0,10001", "0,0,NaN", "0,0" }) Reject(() => WheelPoint.Parse(invalid));
        foreach (var invalid in new[] { "-1", "60001", "NaN" }) Reject(() => Options.ParseInitialDelay(invalid));
        Assert(Options.ParseInitialDelay("12000") == 12000 && Options.ParseInitialDelay("0") == 0, "initial action delay parsing failed");
        var delayed = new Options { Url = "about:blank", Output = "unused.json", ChromePath = "unused.exe", InitialActionDelayMs = 12000, NavigationDelayMs = 1000 };
        Assert(delayed.NextActionDelayMs() == 12000 && delayed.NextActionDelayMs() == 1000, "initial delay leaked into subsequent actions");
        var normal = new Options { Url = "about:blank", Output = "unused.json", ChromePath = "unused.exe", NavigationDelayMs = 1000 };
        Assert(normal.NextActionDelayMs() == 1000 && normal.NextActionDelayMs() == 1000, "default initial delay changed existing action spacing");

        var options = new Options { Url = "about:blank", Output = "unused.json", ChromePath = "unused.exe" };
        var start = ChromeLaunch.Settings(options, "G:/unused-profile");
        Assert(!start.UseShellExecute && start.CreateNoWindow && start.WindowStyle == ProcessWindowStyle.Hidden && start.ArgumentList.Contains("--headless") && start.ArgumentList.Contains("--mute-audio"), "native wheel extension weakened headless launch guards");
        ParseOptions();
        Console.WriteLine("Native wheel timing self-tests passed without browser execution.");
    }

    private static void ParseOptions()
    {
        var basic = new[] { "--url", "https://example.invalid/", "--output", "G:/unused-wheel.json", "--chrome", Environment.ProcessPath! };
        var options = Options.Parse(basic.Concat(new[] { "--wheel-after-ready", "600,300,600", "--wheel-after-ready", "600,300,-600", "--initial-action-delay-ms", "12000", "--navigation-delay-ms", "1000" }).ToArray());
        Assert(options.WheelAfterReady.Count == 2 && options.InitialActionDelayMs == 12000 && options.NavigationDelayMs == 1000, "ordered stage/spacing options were lost");
        Reject(() => Options.Parse(basic.Concat(new[] { "--wheel-after-ready", "600,300,600", "--early-scroll" }).ToArray()));
        Reject(() => Options.Parse(basic.Concat(new[] { "--wheel-after-ready", "600,300,600", "--click-after-ready", "1,1" }).ToArray()));
        Reject(() => Options.Parse(basic.Concat(new[] { "--initial-action-delay-ms", "12000" }).ToArray()));
        Reject(() => Options.Parse(basic.Concat(Enumerable.Range(0, 129).SelectMany(_ => new[] { "--wheel-after-ready", "600,300,600" })).ToArray()));
    }

    private static void Reject(Action action)
    {
        try { action(); }
        catch (ArgumentException) { return; }
        throw new InvalidOperationException("invalid native wheel argument was accepted");
    }
    private static void Assert(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
}
