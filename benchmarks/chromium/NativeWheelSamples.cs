namespace ChromiumBaseline;

internal sealed class NativeWheelResult
{
    public string TimingScope { get; init; } = "CDP reply is not a default-action verdict. First direction-consistent viewport offset frame receipt includes PNG/CDP delivery, may include prior in-flight motion, and is not Breeze retained-paint or display timing. Nested/cancelled/missing-verdict frames are unattributed.";
    public int SampleLimit { get; init; } = 128;
    public int OmittedInputs { get; set; }
    public int RequestedInputs { get; set; }
    public int UnattemptedInputs => Math.Max(0, RequestedInputs - Samples.Count - OmittedInputs);
    public List<NativeWheelSample> Samples { get; } = new();
}

internal sealed class NativeWheelSample
{
    public int Sequence { get; init; }
    public int DocumentEpoch { get; init; }
    public required WheelPoint Input { get; init; }
    public double ScheduledMs { get; init; }
    public double EnqueuedMs { get; init; }
    public double? BaselineScrollY { get; init; }
    public double? FirstChangedFrameScrollY { get; set; }
    public double? EnqueueToCdpReplyMs { get; set; }
    public double? EnqueueToFirstChangedCompositorFrameReceivedMs { get; set; }
    public double? FrameSwapTimestamp { get; set; }
    public double? FrameSwapMonotonicTimestamp { get; set; }
    public bool? DefaultPrevented { get; set; }
    public bool? ObservedNestedScrollPort { get; set; }
    public string? ListenerVerdictReason { get; set; }
    public int? ObservedListenerEvents { get; set; }
    public double? ObservedEventDeltaY { get; set; }
    public bool ListenerVerdictDeadlineReached { get; set; }
    internal double? ObservationDeadlineMs { get; set; }
    public string Status { get; set; } = "unacknowledged";
}

internal sealed class NativeWheelSamples
{
    public NativeWheelResult Result { get; } = new();
    public double? LatestScrollY { get; private set; }
    public int DocumentEpoch { get; private set; }
    private NativeWheelSample? pending;

    public void Navigated()
    {
        if (pending is not null) { pending.Status = "retired_document"; ClearFrame(pending); }
        pending = null;
        LatestScrollY = null;
        DocumentEpoch++;
    }

    public NativeWheelSample? Begin(WheelPoint point, double scheduled, double now)
    {
        if (pending is not null) { pending.Status = "superseded"; ClearFrame(pending); }
        pending = null;
        if (Result.Samples.Count == Result.SampleLimit) { Result.OmittedInputs++; return null; }
        var sample = new NativeWheelSample {
            Sequence = Result.Samples.Count + 1, DocumentEpoch = DocumentEpoch,
            Input = point, ScheduledMs = scheduled, EnqueuedMs = now,
            BaselineScrollY = LatestScrollY
        };
        Result.Samples.Add(sample);
        pending = sample;
        return sample;
    }

    public void Frame(double scrollY, double now, double? timestamp, double? monotonic)
    {
        if (!double.IsFinite(scrollY) || scrollY < 0) return;
        LatestScrollY = scrollY;
        if (pending is not { } sample || !IsPending(sample) || sample.DocumentEpoch != DocumentEpoch || sample.BaselineScrollY is not { } baseline) return;
        if (sample.ObservationDeadlineMs is { } deadline && now >= deadline) return;
        if ((scrollY - baseline) * Math.Sign(sample.Input.Delta) <= 0.001) return;
        sample.EnqueueToFirstChangedCompositorFrameReceivedMs = Math.Max(0, now - sample.EnqueuedMs);
        sample.FirstChangedFrameScrollY = scrollY;
        sample.FrameSwapTimestamp = timestamp;
        sample.FrameSwapMonotonicTimestamp = monotonic;
        sample.Status = "compositor_motion_observed";
    }

    public void Replied(NativeWheelSample sample, double now)
    {
        if (sample.DocumentEpoch != DocumentEpoch || sample.Status == "retired_document") return;
        sample.EnqueueToCdpReplyMs = Math.Max(0, now - sample.EnqueuedMs);
        if (sample.Status == "unacknowledged") sample.Status = "acknowledged_waiting_frame";
    }

    public void Finish(NativeWheelSample sample, NativeWheelVerdict verdict)
    {
        if (sample.DocumentEpoch != DocumentEpoch || sample.Status is "retired_document" or "superseded") return;
        sample.ListenerVerdictReason = verdict.Reason;
        sample.ObservedListenerEvents = verdict.ObservedEvents;
        sample.ObservedEventDeltaY = verdict.ObservedDelta;
        sample.ListenerVerdictDeadlineReached = verdict.DeadlineReached;
        Finish(sample, verdict.DefaultPrevented, verdict.Nested);
    }

    public void Finish(NativeWheelSample sample, bool? cancelled, bool nested)
    {
        if (sample.DocumentEpoch != DocumentEpoch || sample.Status is "retired_document" or "superseded") return;
        sample.DefaultPrevented = cancelled;
        sample.ObservedNestedScrollPort = cancelled is null ? null : nested;
        if (cancelled is null || cancelled == true || nested)
        {
            sample.Status = cancelled is null ? "listener_verdict_missing" : cancelled == true ? "cancelled" : "nested_frame_unattributed";
            ClearFrame(sample);
        }
        else if (sample.EnqueueToCdpReplyMs is null)
        {
            sample.Status = "unacknowledged";
            ClearFrame(sample);
        }
        else if (sample.EnqueueToFirstChangedCompositorFrameReceivedMs is null)
            sample.Status = sample.BaselineScrollY is null ? "no_baseline_frame" : "no_viewport_motion_observed";
        if (ReferenceEquals(pending, sample)) pending = null;
    }

    public void Failed(NativeWheelSample sample)
    {
        if (sample.Status != "retired_document") sample.Status = "dispatch_failed";
        ClearFrame(sample);
        pending = null;
    }
    private static void ClearFrame(NativeWheelSample sample)
    {
        sample.EnqueueToFirstChangedCompositorFrameReceivedMs = null;
        sample.FirstChangedFrameScrollY = null;
        sample.FrameSwapTimestamp = sample.FrameSwapMonotonicTimestamp = null;
    }
    private static bool IsPending(NativeWheelSample sample) => sample.Status is "unacknowledged" or "acknowledged_waiting_frame";
}

internal sealed record WheelPoint(int X, int Y, int Delta)
{
    public static WheelPoint Parse(string value)
    {
        var parts = value.Split(',', StringSplitOptions.TrimEntries);
        if (parts.Length != 3 || !int.TryParse(parts[0], out var x) || !int.TryParse(parts[1], out var y) || !int.TryParse(parts[2], out var delta) || x is < 0 or > 7680 || y is < 0 or > 4320 || delta is < -10000 or > 10000)
            throw new ArgumentException("--wheel-after-ready requires bounded CSS viewport x,y,delta.");
        return new(x, y, delta);
    }
}
