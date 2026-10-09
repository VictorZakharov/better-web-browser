using ChromiumBaseline;
using System.Text.Json;

internal static class ProcessMemoryTests
{
    internal static void Run()
    {
        var sample = Sample(new[] {
            new OwnedProcessSample(10, 100, 80, 130, 20),
            new OwnedProcessSample(11, 200, 160, 230, 40),
            new OwnedProcessSample(12, 300, 240, 330, 60),
            new OwnedProcessSample(13, 400, 320, 430, 80),
            new OwnedProcessSample(14, 500, 400, 530, 100)
        });
        var breakdown = Parse(sample, """
            {"processInfo":[
                {"id":10,"type":"browser","cpuTime":99999},
                {"id":11,"type":"renderer","cpuTime":0},
                {"id":12,"type":"renderer"},
                {"id":13,"type":"GPU"},
                {"id":99999,"type":"renderer"}
            ]}
            """);
        Check(breakdown.AttributionError is null, "valid role attribution failed");
        var renderer = breakdown.Roles.Single(role => role.Role == "renderer");
        Check(renderer.ProcessCount == 2 && renderer.PrivateBytes == 400 &&
            renderer.WorkingSetBytes == 500 && renderer.CpuTimeMs == 100,
            "renderer memory was conflated with browser/GPU/unowned CDP processes");
        Check(breakdown.Roles.Single(role => role.Role == "gpu").PrivateBytes == 320,
            "GPU process was not independently attributed");
        Check(breakdown.Roles.Single(role => role.Role == "unattributed").PrivateBytes == 400,
            "missing process role was reported as zero or silently dropped");
        Totals(sample, breakdown);
        var serialized = JsonSerializer.Serialize(breakdown, JsonDefaults.Options);
        Check(!serialized.Contains("99999") && !serialized.Contains("\"id\""),
            "role aggregates leaked unrelated/process identity records");

        var unknown = Parse(sample, """
            {"processInfo":[{"id":10,"type":"author-url-or-command-line"}]}
            """);
        Check(unknown.Roles.Single(role => role.Role == "other").ProcessCount == 1,
            "unknown type did not use the fixed other category");
        Check(!JsonSerializer.Serialize(unknown).Contains("author-url"),
            "arbitrary upstream role text escaped the fixed category set");
        Totals(sample, unknown);

        foreach (var invalid in new[] {
            "null", "[]", "{}", "{\"processInfo\":null}",
            "{\"processInfo\":[null]}",
            "{\"processInfo\":[{\"id\":0,\"type\":\"renderer\"}]}",
            "{\"processInfo\":[{\"id\":-1,\"type\":\"renderer\"}]}",
            "{\"processInfo\":[{\"id\":2147483648,\"type\":\"renderer\"}]}",
            "{\"processInfo\":[{\"id\":\"10\",\"type\":\"renderer\"}]}",
            "{\"processInfo\":[{\"id\":10,\"type\":null}]}",
            "{\"processInfo\":[{\"id\":10,\"type\":\"renderer\"},{\"id\":10,\"type\":\"GPU\"}]}"
        }) {
            var result = Parse(sample, invalid);
            Check(result.AttributionError is not null &&
                result.Roles.Single().Role == "unattributed",
                "malformed role reply partially or silently attributed memory");
            Totals(sample, result);
        }
        var tooMany = "{\"processInfo\":[" + string.Join(",",
            Enumerable.Range(1, 1025).Select(id => $"{{\"id\":{id},\"type\":\"renderer\"}}")) + "]}";
        Check(Parse(sample, tooMany).AttributionError is not null,
            "unbounded process role reply was accepted");
        var absent = ProcessMemoryBreakdown.From(sample, null, "test unavailable");
        Check(absent.AttributionError == "test unavailable" &&
            absent.Roles.Single().Role == "unattributed", "unavailable query fabricated attribution");
        Totals(sample, absent);
        var empty = Parse(Sample(Array.Empty<OwnedProcessSample>()), "{\"processInfo\":[]}");
        Check(empty.Roles.Count == 0 && empty.AttributionError is null,
            "empty owned snapshot invented a process");
        Console.WriteLine("Chrome process-role memory attribution tests passed without browser execution.");
    }

    private static ProcessSample Sample(IReadOnlyList<OwnedProcessSample> processes) => new(
        processes.Sum(process => process.WorkingSetBytes),
        processes.Sum(process => process.PrivateBytes),
        processes.Sum(process => process.PeakWorkingSetBytes),
        processes.Sum(process => process.CpuTimeMs), processes.Count, processes);

    private static ProcessMemoryBreakdown Parse(ProcessSample sample, string json)
    {
        using var document = JsonDocument.Parse(json);
        return ProcessMemoryBreakdown.From(sample, document.RootElement);
    }

    private static void Totals(ProcessSample sample, ProcessMemoryBreakdown breakdown)
    {
        Check(breakdown.Roles.Sum(role => role.ProcessCount) == sample.ProcessCount &&
            breakdown.Roles.Sum(role => role.PrivateBytes) == sample.PrivateBytes &&
            breakdown.Roles.Sum(role => role.WorkingSetBytes) == sample.WorkingSetBytes &&
            breakdown.Roles.Sum(role => role.SumOfProcessPeakWorkingSetBytes) == sample.PeakWorkingSetBytes &&
            breakdown.Roles.Sum(role => role.CpuTimeMs) == sample.CpuTimeMs,
            "role rows did not partition the same owned OS sample as the aggregate");
    }

    private static void Check(bool success, string message)
    {
        if (!success) throw new InvalidOperationException(message);
    }
}
