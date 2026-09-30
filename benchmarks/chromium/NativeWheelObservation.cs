namespace ChromiumBaseline;

// The compositor frame and main-thread listener are independent observations.
// Both share one budget beginning at dispatch acknowledgement, not one budget each.
internal sealed class NativeWheelObservation(double acknowledgedMs, int spacingMs)
{
    public double DeadlineMs { get; } = acknowledgedMs + Math.Clamp(spacingMs * 3 / 4, 100, 1000);
    public double RemainingMs(double now) => Math.Max(0, DeadlineMs - now);
    public double PollDelayMs(double now) => Math.Min(20, RemainingMs(now));
    public bool Accepts(NativeWheelVerdict verdict, double now) =>
        now < DeadlineMs && verdict.DefaultPrevented is not null;

    public NativeWheelVerdict Expired(NativeWheelVerdict verdict) => verdict with {
        DefaultPrevented = null, Nested = false, DeadlineReached = true,
        Reason = verdict.DefaultPrevented is null ? verdict.Reason : "verdict_arrived_after_deadline"
    };
}
