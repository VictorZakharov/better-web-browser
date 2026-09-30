using System.Text.Json;

namespace ChromiumBaseline;

// Missing observations are not cancellation or no-motion evidence. Keep exact event
// sequence/delta admission and report why the isolated observer could not certify it.
internal sealed record NativeWheelVerdict(
    bool? DefaultPrevented, bool Nested, string Reason, int? ObservedEvents, double? ObservedDelta,
    bool DeadlineReached = false)
{
    public static NativeWheelVerdict Parse(JsonElement events, int sequence, int expectedDelta)
    {
        var result = new NativeWheelVerdict(null, false, "probe_value_missing", null, null);
        if (events.ValueKind != JsonValueKind.Array) return result;
        var count = events.GetArrayLength();
        result = result with { ObservedEvents = count, Reason = "event_sequence_missing" };
        if (sequence < 1 || sequence > count) return result;
        var item = events[sequence - 1];
        result = result with { Reason = "event_verdict_invalid" };
        if (item.ValueKind != JsonValueKind.Object || !item.TryGetProperty("deltaY", out var delta) ||
            delta.ValueKind != JsonValueKind.Number || !delta.TryGetDouble(out var number) || !double.IsFinite(number))
            return result;
        result = result with { ObservedDelta = number };
        if (number != expectedDelta) return result with { Reason = "event_delta_mismatch" };
        if (!item.TryGetProperty("defaultPrevented", out var prevented)) return result;
        if (prevented.ValueKind == JsonValueKind.Null) return result with { Reason = "default_prevented_pending" };
        if (prevented.ValueKind is not (JsonValueKind.True or JsonValueKind.False) ||
            !item.TryGetProperty("nested", out var nested) || nested.ValueKind is not (JsonValueKind.True or JsonValueKind.False))
            return result;
        return result with { DefaultPrevented = prevented.GetBoolean(), Nested = nested.GetBoolean(), Reason = "observed" };
    }
}
