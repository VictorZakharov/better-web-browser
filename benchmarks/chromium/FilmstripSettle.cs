using System.Text.Json;

namespace ChromiumBaseline;

/// <summary>
/// Settle times of a hidden filmstrip directory (<c>manifest.json</c> and its frame PNGs), as
/// both Breeze's <c>--filmstrip-directory</c> and this harness write them, so the two browsers are
/// measured by one rule. Times are the frames' scheduled offsets from navigation start, so their
/// resolution is the capture interval.
/// </summary>
/// <param name="FirstContentMs">
/// The first frame whose content area is not one uniform color (a blank page of any color).
/// </param>
/// <param name="RegionSettledMs">
/// The frame from which the optional region stops changing, when the region has content then.
/// </param>
/// <param name="LastChangeMs">The last frame whose content area differs from the one before.</param>
/// <param name="Content">
/// The page area compared, which excludes browser chrome such as Breeze's toolbar and status
/// bar; the whole frame when not given.
/// </param>
internal sealed record FilmstripSettle(
    int Frames,
    double CapturedToMs,
    double? FirstContentMs,
    double? RegionSettledMs,
    double? LastChangeMs,
    string? Content,
    string? Region);

/// <summary>A rectangle in screenshot pixels, given as <c>x,y,width,height</c>.</summary>
internal readonly record struct PixelRegion(int X, int Y, int Width, int Height)
{
    public static PixelRegion Parse(string value)
    {
        var parts = value.Split(',');
        if (parts.Length != 4 || !parts.All(part => int.TryParse(part, out var number) && number >= 0))
            throw new ArgumentException("--region must be x,y,width,height in screenshot pixels.");
        var numbers = parts.Select(int.Parse).ToArray();
        if (numbers[2] == 0 || numbers[3] == 0)
            throw new ArgumentException("--region must have a positive width and height.");
        return new PixelRegion(numbers[0], numbers[1], numbers[2], numbers[3]);
    }

    public override string ToString() => $"{X},{Y},{Width},{Height}";
}

internal static class FilmstripSettleAnalysis
{
    /// <summary>
    /// A frame changes when more than this share of its pixels differs from the previous frame's
    /// by more than <see cref="ChannelTolerance"/> in a channel; antialiasing and caret noise stay
    /// below it.
    /// </summary>
    private const double ChangedShare = 0.0005;
    private const int ChannelTolerance = 16;

    public static FilmstripSettle Analyze(string directory, PixelRegion? content, PixelRegion? region)
    {
        using var manifest = JsonDocument.Parse(File.ReadAllText(Path.Combine(directory, "manifest.json")));
        var frames = manifest.RootElement.GetProperty("frames").EnumerateArray()
            .Where(frame => !frame.TryGetProperty("error", out var error) || error.ValueKind == JsonValueKind.Null)
            .Select(frame => (Ms: frame.GetProperty("scheduled_ms").GetDouble(),
                File: frame.GetProperty("file").GetString()!))
            .OrderBy(frame => frame.Ms)
            .ToArray();
        double? firstContent = null, lastChange = null, regionSettled = null;
        var regionHasContent = false;
        PngImage? previous = null;
        foreach (var (ms, file) in frames)
        {
            var image = PngImage.Decode(File.ReadAllBytes(Path.Combine(directory, file)));
            var page = content ?? new PixelRegion(0, 0, image.Width, image.Height);
            if (firstContent is null && HasContent(image, page)) firstContent = ms;
            if (previous is not null && Changed(previous, image, page)) lastChange = ms;
            if (region is { } area)
            {
                var hasContent = HasContent(image, area);
                if (previous is null || Changed(previous, image, area) || hasContent != regionHasContent)
                    regionSettled = ms;
                regionHasContent = hasContent;
            }
            previous = image;
        }
        if (firstContent is { } first && (lastChange is null || lastChange < first)) lastChange = first;
        return new FilmstripSettle(frames.Length, frames.Length == 0 ? 0 : frames[^1].Ms, firstContent,
            regionHasContent ? regionSettled : null, lastChange, content?.ToString(), region?.ToString());
    }

    /// <summary>Whether more than a noise share of the region differs from its dominant color.</summary>
    private static bool HasContent(PngImage image, PixelRegion region)
    {
        var rgb = image.Rgb;
        var buckets = new int[32 * 32 * 32];
        long total = 0;
        foreach (var offset in Pixels(image, region))
        {
            total++;
            buckets[(rgb[offset] >> 3) << 10 | (rgb[offset + 1] >> 3) << 5 | rgb[offset + 2] >> 3]++;
        }
        if (total == 0) return false;
        var dominant = Array.IndexOf(buckets, buckets.Max());
        var (red, green, blue) = ((dominant >> 10) * 8 + 4, ((dominant >> 5) & 31) * 8 + 4, (dominant & 31) * 8 + 4);
        long differing = 0;
        foreach (var offset in Pixels(image, region))
        {
            if (Math.Abs(rgb[offset] - red) > ChannelTolerance ||
                Math.Abs(rgb[offset + 1] - green) > ChannelTolerance ||
                Math.Abs(rgb[offset + 2] - blue) > ChannelTolerance)
                differing++;
        }
        return differing > total * ChangedShare;
    }

    private static bool Changed(PngImage previous, PngImage current, PixelRegion region)
    {
        if (previous.Width != current.Width || previous.Height != current.Height) return true;
        long changed = 0, total = 0;
        foreach (var offset in Pixels(current, region))
        {
            total++;
            if (Math.Abs(previous.Rgb[offset] - current.Rgb[offset]) > ChannelTolerance ||
                Math.Abs(previous.Rgb[offset + 1] - current.Rgb[offset + 1]) > ChannelTolerance ||
                Math.Abs(previous.Rgb[offset + 2] - current.Rgb[offset + 2]) > ChannelTolerance)
                changed++;
        }
        return total > 0 && changed > total * ChangedShare;
    }

    /// <summary>RGB offsets of the pixels of <paramref name="region"/> inside the image.</summary>
    private static IEnumerable<int> Pixels(PngImage image, PixelRegion region)
    {
        var right = Math.Min(image.Width, region.X + region.Width);
        var bottom = Math.Min(image.Height, region.Y + region.Height);
        for (var y = region.Y; y < bottom; y++)
            for (var x = region.X; x < right; x++)
                yield return (y * image.Width + x) * 3;
    }
}
