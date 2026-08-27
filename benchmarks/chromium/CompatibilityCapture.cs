using System.Diagnostics;
using System.Text.Json;

namespace ChromiumBaseline;

internal sealed class CompatibilityCapture
{
    public required byte[] ScreenshotPng { get; init; }
    public int NextCommandId { get; init; }
    public int ComposedTextLength { get; init; }
    public int ComposedElementCount { get; init; }
    public int ShadowRootCount { get; init; }
    public int LayoutNodeCount { get; init; }
    public int AccessibilityTextLength { get; init; }
    public int AccessibilityNodeCount { get; init; }
    public int ScreenshotDistinctColors { get; init; }
    public double PaintedPixelRatio { get; init; }
    public double ScreenshotCaptureMs { get; init; }

    public static async Task<CompatibilityCapture> CollectAsync(
        CdpConnection cdp,
        int firstCommandId,
        TimeSpan timeout)
    {
        var nextId = firstCommandId;
        await cdp.CallAsync(nextId++, "DOMSnapshot.enable", null, timeout);
        var snapshot = await cdp.CallAsync(nextId++, "DOMSnapshot.captureSnapshot", new
        {
            computedStyles = Array.Empty<string>(),
            includeDOMRects = false
        }, timeout);
        var domTree = await cdp.CallAsync(nextId++, "DOM.getDocument", new
        {
            depth = -1,
            pierce = true
        }, timeout);
        await cdp.CallAsync(nextId++, "DOMSnapshot.disable", null, timeout);

        await cdp.CallAsync(nextId++, "Accessibility.enable", null, timeout);
        var accessibility = await cdp.CallAsync(nextId++, "Accessibility.getFullAXTree", null, timeout);
        await cdp.CallAsync(nextId++, "Accessibility.disable", null, timeout);

        var screenshotStarted = Stopwatch.GetTimestamp();
        var screenshot = await cdp.CallAsync(nextId++, "Page.captureScreenshot", new
        {
            format = "png",
            fromSurface = true,
            captureBeyondViewport = false
        }, timeout);
        var screenshotCaptureMs = Stopwatch.GetElapsedTime(screenshotStarted).TotalMilliseconds;
        var screenshotPng = Convert.FromBase64String(screenshot.GetProperty("data").GetString()!);
        var paint = ScreenshotEvidence.Analyze(screenshotPng);
        var dom = ParseDomSnapshot(snapshot, CountAuthoredShadowRoots(domTree.GetProperty("root")));
        var ax = ParseAccessibilityTree(accessibility);
        return new CompatibilityCapture
        {
            ScreenshotPng = screenshotPng,
            NextCommandId = nextId,
            ComposedTextLength = dom.TextLength,
            ComposedElementCount = dom.ElementCount,
            ShadowRootCount = dom.ShadowRootCount,
            LayoutNodeCount = dom.LayoutNodeCount,
            AccessibilityTextLength = ax.TextLength,
            AccessibilityNodeCount = ax.NodeCount,
            ScreenshotDistinctColors = paint.DistinctColors,
            PaintedPixelRatio = paint.PaintedPixelRatio,
            ScreenshotCaptureMs = screenshotCaptureMs
        };
    }

    public void ApplyTo(BenchmarkResult result)
    {
        result.ComposedTextLength = ComposedTextLength;
        result.ComposedElementCount = ComposedElementCount;
        result.ShadowRootCount = ShadowRootCount;
        result.LayoutNodeCount = LayoutNodeCount;
        result.AccessibilityTextLength = AccessibilityTextLength;
        result.AccessibilityNodeCount = AccessibilityNodeCount;
        result.ScreenshotDistinctColors = ScreenshotDistinctColors;
        result.PaintedPixelRatio = PaintedPixelRatio;
        result.PaintCaptureMs = ScreenshotCaptureMs;
    }

    private static DomMetrics ParseDomSnapshot(JsonElement snapshot, int shadowRootCount)
    {
        var strings = snapshot.GetProperty("strings")
            .EnumerateArray()
            .Select(value => value.GetString() ?? string.Empty)
            .ToArray();
        var metrics = new DomMetrics { ShadowRootCount = shadowRootCount };
        foreach (var document in snapshot.GetProperty("documents").EnumerateArray())
        {
            var nodes = document.GetProperty("nodes");
            var nodeTypes = nodes.GetProperty("nodeType")
                .EnumerateArray()
                .Select(value => value.GetInt32())
                .ToArray();
            metrics.ElementCount += nodeTypes.Count(value => value == 1);
            var layout = document.GetProperty("layout");
            metrics.LayoutNodeCount += layout.GetProperty("nodeIndex").GetArrayLength();
            foreach (var textIndex in layout.GetProperty("text").EnumerateArray())
            {
                var index = textIndex.GetInt32();
                if ((uint)index < (uint)strings.Length)
                {
                    metrics.TextLength += strings[index].Trim().Length;
                }
            }
        }
        return metrics;
    }

    private static int CountAuthoredShadowRoots(JsonElement node)
    {
        var count = 0;
        var pending = new Stack<JsonElement>();
        pending.Push(node);
        while (pending.TryPop(out var current))
        {
            if (current.TryGetProperty("shadowRoots", out var shadowRoots))
            {
                foreach (var shadowRoot in shadowRoots.EnumerateArray())
                {
                    var type = shadowRoot.TryGetProperty("shadowRootType", out var value)
                        ? value.GetString()
                        : null;
                    if (type is "open" or "closed")
                    {
                        count++;
                    }
                    pending.Push(shadowRoot);
                }
            }
            if (current.TryGetProperty("children", out var children))
            {
                foreach (var child in children.EnumerateArray())
                {
                    pending.Push(child);
                }
            }
            if (current.TryGetProperty("contentDocument", out var contentDocument))
            {
                pending.Push(contentDocument);
            }
            if (current.TryGetProperty("templateContent", out var templateContent))
            {
                pending.Push(templateContent);
            }
        }
        return count;
    }

    private static AccessibilityMetrics ParseAccessibilityTree(JsonElement accessibility)
    {
        var metrics = new AccessibilityMetrics();
        foreach (var node in accessibility.GetProperty("nodes").EnumerateArray())
        {
            if (node.TryGetProperty("ignored", out var ignored) && ignored.GetBoolean())
            {
                continue;
            }
            var role = AxValue(node, "role");
            if (role is "RootWebArea" or "none" or "generic")
            {
                continue;
            }
            metrics.NodeCount++;
            metrics.TextLength += AxValue(node, "name").Trim().Length;
        }
        return metrics;
    }

    private static string AxValue(JsonElement node, string property) =>
        node.TryGetProperty(property, out var wrapper) &&
        wrapper.TryGetProperty("value", out var value) &&
        value.ValueKind == JsonValueKind.String
            ? value.GetString() ?? string.Empty
            : string.Empty;

    private sealed class DomMetrics
    {
        public int TextLength { get; set; }
        public int ElementCount { get; set; }
        public int ShadowRootCount { get; set; }
        public int LayoutNodeCount { get; set; }
    }

    private sealed class AccessibilityMetrics
    {
        public int TextLength { get; set; }
        public int NodeCount { get; set; }
    }
}
