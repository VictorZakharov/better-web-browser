using System.Text.Json;

namespace ChromiumBaseline;

internal sealed class ProcessMemoryBreakdown
{
    public string Source { get; init; } = "owned_windows_process_samples_and_cdp_system_info";
    public string? AttributionError { get; init; }
    public IReadOnlyList<ProcessRoleMemory> Roles { get; init; } = Array.Empty<ProcessRoleMemory>();

    internal static ProcessMemoryBreakdown From(ProcessSample sample, JsonElement? reply,
        string? error = null)
    {
        var roles = new Dictionary<int, string>();
        try
        {
            if (reply is { } response) roles = ReadRoles(response);
        }
        catch (InvalidOperationException failure) { error ??= failure.Message; }
        // CDP supplies labels only. Never sample/open a PID because CDP named it:
        // the OS snapshot has already established ownership independently.
        var grouped = sample.Processes.GroupBy(process =>
            roles.GetValueOrDefault(process.Id, "unattributed"));
        return new ProcessMemoryBreakdown {
            AttributionError = error,
            Roles = grouped.OrderBy(group => group.Key, StringComparer.Ordinal).Select(group =>
                new ProcessRoleMemory {
                    Role = group.Key,
                    ProcessCount = group.Count(),
                    PrivateBytes = group.Sum(process => process.PrivateBytes),
                    WorkingSetBytes = group.Sum(process => process.WorkingSetBytes),
                    // Sum of per-process lifetime peaks, not a simultaneous tree peak.
                    SumOfProcessPeakWorkingSetBytes = group.Sum(process => process.PeakWorkingSetBytes),
                    CpuTimeMs = group.Sum(process => process.CpuTimeMs)
                }).ToArray()
        };
    }

    private static Dictionary<int, string> ReadRoles(JsonElement response)
    {
        if (response.ValueKind != JsonValueKind.Object ||
            !response.TryGetProperty("processInfo", out var entries) ||
            entries.ValueKind != JsonValueKind.Array || entries.GetArrayLength() > 1024)
            throw new InvalidOperationException("CDP process role list is unavailable or exceeds its bound.");
        var roles = new Dictionary<int, string>();
        foreach (var entry in entries.EnumerateArray())
        {
            if (entry.ValueKind != JsonValueKind.Object ||
                !entry.TryGetProperty("id", out var id) || !id.TryGetInt32(out var pid) || pid <= 0 ||
                !entry.TryGetProperty("type", out var type) || type.ValueKind != JsonValueKind.String)
                throw new InvalidOperationException("CDP process role entry is malformed.");
            var role = type.GetString() switch {
                "browser" => "browser", "renderer" => "renderer", "GPU" => "gpu",
                "utility" => "utility", _ => "other"
            };
            if (!roles.TryAdd(pid, role))
                throw new InvalidOperationException("CDP process role list contains duplicate identities.");
        }
        return roles;
    }
}

internal sealed class ProcessRoleMemory
{
    public string Role { get; init; } = "unattributed";
    public int ProcessCount { get; init; }
    public long PrivateBytes { get; init; }
    public long WorkingSetBytes { get; init; }
    public long SumOfProcessPeakWorkingSetBytes { get; init; }
    public double CpuTimeMs { get; init; }
}
