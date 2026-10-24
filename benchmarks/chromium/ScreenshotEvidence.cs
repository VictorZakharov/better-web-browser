namespace ChromiumBaseline;

internal readonly record struct ScreenshotEvidence(int DistinctColors, double PaintedPixelRatio)
{
    public static ScreenshotEvidence Analyze(byte[] png)
    {
        var image = PngImage.Decode(png);
        var colors = new bool[32 * 32 * 32];
        var distinct = 0;
        long painted = 0;
        var rgb = image.Rgb;
        for (var pixel = 0; pixel < rgb.Length; pixel += 3)
        {
            var (red, green, blue) = (rgb[pixel], rgb[pixel + 1], rgb[pixel + 2]);
            var bucket = (red >> 3) << 10 | (green >> 3) << 5 | blue >> 3;
            if (!colors[bucket])
            {
                colors[bucket] = true;
                distinct++;
            }
            if (PngImage.IsPainted(rgb, pixel))
            {
                painted++;
            }
        }

        return new ScreenshotEvidence(distinct, painted / (double)(image.Width * image.Height));
    }
}
