using System.Diagnostics;
using System.Text.Json;

namespace ChromiumBaseline;

// Sample compositor-delivered frames, including unchanged frames, on a wall-clock grid.
// A synchronous capture requested before the first paint can stall on Windows Chrome.
// Screencast avoids forcing layout or postponing the early samples until page load.
internal sealed class FilmstripCapture : IDisposable
{
    private readonly CdpConnection cdp;
    private readonly Options options;
    private readonly Stopwatch stopwatch;
    private readonly object gate = new();
    private readonly IDisposable subscription;
    private (string Data, TimeSpan Time) latest;
    private Task acknowledgements = Task.CompletedTask;
    private int nextId = 50_010;

    private FilmstripCapture(CdpConnection cdp, Options options, Stopwatch stopwatch, string initial)
    {
        this.cdp = cdp;
        this.options = options;
        this.stopwatch = stopwatch;
        latest = (initial, stopwatch.Elapsed);
        subscription = cdp.Observe(root =>
        {
            if (!root.TryGetProperty("method", out var method) || method.GetString() != "Page.screencastFrame")
                return;
            var frame = root.GetProperty("params");
            var sessionId = frame.GetProperty("sessionId").GetInt32();
            lock (gate)
            {
                latest = (frame.GetProperty("data").GetString()!, stopwatch.Elapsed);
                acknowledgements = AckAsync(acknowledgements, nextId++, sessionId);
            }
        });
    }

    public static async Task<FilmstripCapture?> StartAsync(CdpConnection cdp, Options options,
        Stopwatch stopwatch, TimeSpan timeout)
    {
        if (options.FilmstripDirectory is null) return null;
        var initial = await cdp.CallAsync(50_000, "Page.captureScreenshot", new { format = "png" }, timeout);
        var capture = new FilmstripCapture(cdp, options, stopwatch, initial.GetProperty("data").GetString()!);
        try
        {
            await cdp.CallAsync(50_001, "Page.startScreencast", new { format = "png", everyNthFrame = 1 }, timeout);
            return capture;
        }
        catch { capture.Dispose(); throw; }
    }

    private async Task AckAsync(Task previous, int id, int sessionId)
    {
        await previous;
        await cdp.CallAsync(id, "Page.screencastFrameAck", new { sessionId }, TimeSpan.FromMilliseconds(options.TimeoutMs));
    }

    public async Task RunAsync(TimeSpan navigationStarted, TimeSpan timeout)
    {
        var directory = options.FilmstripDirectory!;
        Directory.CreateDirectory(directory);
        var frames = new List<object>();
        try
        {
            for (var index = 1; index <= options.FilmstripDurationMs / options.FilmstripIntervalMs; index++)
            {
                var scheduled = checked(index * options.FilmstripIntervalMs);
                var delay = navigationStarted + TimeSpan.FromMilliseconds(scheduled) - stopwatch.Elapsed;
                if (delay > TimeSpan.Zero) await Task.Delay(delay);
                (string Data, TimeSpan Time) frame;
                lock (gate) { frame = latest; }
                var sampled = (stopwatch.Elapsed - navigationStarted).TotalMilliseconds;
                var file = $"frame-{scheduled:000000}ms.png";
                await File.WriteAllBytesAsync(Path.Combine(directory, file), Convert.FromBase64String(frame.Data));
                frames.Add(new { scheduled_ms = scheduled, captured_ms = sampled,
                    source_frame_ms = (frame.Time - navigationStarted).TotalMilliseconds, file, error = (string?)null });
                await File.WriteAllTextAsync(Path.Combine(directory, "manifest.json"), JsonSerializer.Serialize(new {
                    anchor = "navigation_start", source = "compositor_screencast_latest_frame",
                    interval_ms = options.FilmstripIntervalMs, duration_ms = options.FilmstripDurationMs, frames
                }, JsonDefaults.Options));
            }
        }
        finally
        {
            await cdp.CallAsync(50_002, "Page.stopScreencast", null, timeout);
            Dispose();
            await acknowledgements;
        }
    }

    public void Dispose() => subscription.Dispose();
}
