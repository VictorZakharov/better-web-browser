using ChromiumBaseline;

internal static class NativeWheelObservationTests
{
    public static void Run()
    {
        var budget = new NativeWheelObservation(10, 1000);
        var ready = new NativeWheelVerdict(false, false, "observed", 1, 600);
        var missing = new NativeWheelVerdict(null, false, "event_sequence_missing", 0, null);
        Assert(budget.DeadlineMs == 760 && budget.RemainingMs(10) == 750, "budget did not start at the acknowledgement");
        Assert(new NativeWheelObservation(10, 0).DeadlineMs == 110 &&
            new NativeWheelObservation(10, 60000).DeadlineMs == 1010, "observation budget bounds changed");
        Assert(!budget.Accepts(missing, 20) && budget.Accepts(ready, 759.999), "readiness ignored exact admitted verdict");
        Assert(!budget.Accepts(ready, 760) && !budget.Accepts(ready, 800), "late verdict gained ownership");
        Assert(budget.PollDelayMs(10) == 20 && budget.PollDelayMs(755) == 5 &&
            budget.PollDelayMs(760) == 0 && budget.RemainingMs(800) == 0, "polling extended/reset the absolute budget");
        var late = budget.Expired(ready);
        Assert(late.DefaultPrevented is null && late.DeadlineReached && late.ObservedEvents == 1 &&
            late.ObservedDelta == 600 && late.Reason == "verdict_arrived_after_deadline", "late response fabricated a final decision");
        Assert(budget.Expired(missing) == (missing with { DeadlineReached = true }), "expiry lost missing-event evidence");

        var samples = new NativeWheelSamples();
        samples.Navigated();
        samples.Frame(0, 0, null, null);
        var sample = samples.Begin(new(600, 300, 600), 0, 0)!;
        samples.Replied(sample, 10);
        sample.ObservationDeadlineMs = budget.DeadlineMs;
        samples.Frame(600, 760, null, null);
        Assert(sample.EnqueueToFirstChangedCompositorFrameReceivedMs is null, "late frame satisfied an expired observation");
        samples.Finish(sample, ready);
        var observed = samples.Begin(new(600, 300, 600), 1000, 1000)!;
        samples.Replied(observed, 1010);
        observed.ObservationDeadlineMs = new NativeWheelObservation(1010, 1000).DeadlineMs;
        samples.Frame(1200, 1040, null, null);
        samples.Frame(1300, 1045, null, null);
        samples.Finish(observed, ready);
        Assert(observed.EnqueueToFirstChangedCompositorFrameReceivedMs == 40 && observed.FirstChangedFrameScrollY == 1200,
            "listener acquisition replaced the original first-frame receipt");
        var unobserved = samples.Begin(new(600, 300, 600), 2000, 2000)!;
        samples.Replied(unobserved, 2010);
        samples.Frame(1900, 2020, null, null);
        samples.Finish(unobserved, budget.Expired(missing));
        Assert(unobserved.ListenerVerdictDeadlineReached && unobserved.ObservedListenerEvents == 0 &&
            unobserved.Status == "listener_verdict_missing" && unobserved.DefaultPrevented is null &&
            unobserved.EnqueueToFirstChangedCompositorFrameReceivedMs is null, "deadline converted missing listener to motion/cancellation");
    }

    private static void Assert(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
