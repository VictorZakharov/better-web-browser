using System.Text.Json;

namespace ChromiumBaseline;

internal static class JsonDefaults
{
    public static JsonSerializerOptions Options { get; } = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower
    };
}

internal sealed class BenchmarkResult
{
    public string Browser { get; init; } = "chromium";
    public bool Headless { get; init; } = true;
    public bool UnifiedHeadless { get; init; } = true;
    public string ChromeVersion { get; set; } = string.Empty;
    public string RequestedUrl { get; init; } = string.Empty;
    public string FinalUrl { get; set; } = string.Empty;
    public string? Error { get; set; }
    public int HttpStatus { get; set; }
    public int RequestedViewportWidthCssPx { get; init; }
    public int RequestedViewportHeightCssPx { get; init; }
    public int ViewportWidthCssPx { get; set; }
    public int ViewportHeightCssPx { get; set; }
    public double DeviceScaleFactor { get; init; }
    public string Locale { get; init; } = string.Empty;
    public bool FreshProfile { get; init; } = true;
    public bool CacheDisabled { get; init; } = true;
    public double WindowReadyMs { get; set; }
    public double PageReadyMs { get; set; }
    public double NavigationMs { get; set; }
    public double FirstUsablePaintMs { get; set; }
    public long SettleMs { get; init; }
    public double JavascriptMs { get; set; }
    public double StyleRefreshMs { get; set; }
    public double LayoutMs { get; set; }
    public double TaskMs { get; set; }
    public double PaintCaptureMs { get; set; }
    public ScrollMetrics SteadyScroll { get; set; } = new();
    public ScrollMetrics? EarlyScroll { get; set; }
    public long WorkingSetBytes { get; set; }
    public long PrivateBytes { get; set; }
    public long PeakWorkingSetBytes { get; set; }
    public double CpuTimeMs { get; set; }
    public double AverageCpuPercent { get; set; }
    public int ProcessCount { get; set; }
    public int BodyTextLength { get; set; }
    public int ElementCount { get; set; }
    public int DocumentHeightCssPx { get; set; }
    public bool FixtureReady { get; set; }
    public string? Screenshot { get; set; }
}

internal sealed class ScrollMetrics
{
    public int Samples { get; set; }
    public double AverageMs { get; set; }
    public double P95Ms { get; set; }
    public double MaximumMs { get; set; }
    public int LongFrames { get; set; }
    public int FinalScrollY { get; set; }
}
