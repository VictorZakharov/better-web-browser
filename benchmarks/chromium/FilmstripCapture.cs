using System.Diagnostics;
using System.Text.Json;

namespace ChromiumBaseline;

internal static class FilmstripCapture
{
    public static async Task RunAsync(
        CdpConnection cdp,
        Options options,
        Stopwatch stopwatch,
        TimeSpan navigationStarted,
        TimeSpan timeout)
    {
        var directory = options.FilmstripDirectory
            ?? throw new InvalidOperationException("Filmstrip directory is unavailable.");
        Directory.CreateDirectory(directory);
        var frames = new List<FilmstripFrame>();
        var frameCount = options.FilmstripDurationMs / options.FilmstripIntervalMs;
        for (var index = 1; index <= frameCount; index++)
        {
            var scheduledMs = checked(index * options.FilmstripIntervalMs);
            var deadline = navigationStarted + TimeSpan.FromMilliseconds(scheduledMs);
            var delay = deadline - stopwatch.Elapsed;
            if (delay > TimeSpan.Zero)
            {
                await Task.Delay(delay);
            }

            var file = $"frame-{scheduledMs:000000}ms.png";
            string? error = null;
            try
            {
                var response = await cdp.CallAsync(
                    50_000 + index,
                    "Page.captureScreenshot",
                    new
                    {
                        format = "png",
                        fromSurface = true,
                        captureBeyondViewport = false
                    },
                    timeout);
                await File.WriteAllBytesAsync(
                    Path.Combine(directory, file),
                    Convert.FromBase64String(response.GetProperty("data").GetString() ?? string.Empty));
            }
            catch (Exception exception)
            {
                error = exception.Message;
            }
            frames.Add(new FilmstripFrame
            {
                ScheduledMs = scheduledMs,
                CapturedMs = (stopwatch.Elapsed - navigationStarted).TotalMilliseconds,
                File = file,
                Error = error
            });
            await WriteManifestAsync(directory, options, frames);
        }
    }

    private static Task WriteManifestAsync(
        string directory,
        Options options,
        IReadOnlyList<FilmstripFrame> frames) =>
        File.WriteAllTextAsync(
            Path.Combine(directory, "manifest.json"),
            JsonSerializer.Serialize(
                new
                {
                    anchor = "navigation_start",
                    interval_ms = options.FilmstripIntervalMs,
                    duration_ms = options.FilmstripDurationMs,
                    frames
                },
                JsonDefaults.Options));

    private sealed class FilmstripFrame
    {
        public int ScheduledMs { get; init; }
        public double CapturedMs { get; init; }
        public required string File { get; init; }
        public string? Error { get; init; }
    }
}
