using ChromiumBaseline;
using System.Buffers.Binary;
using System.IO.Compression;
using System.Text.Json;

internal static class FilmstripSettleTests
{
    private const int Width = 40;
    private const int Height = 20;

    public static void Run()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"breeze-settle-tests-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            // 100 ms a uniform dark page, 200 ms text, 300 ms unchanged, 400 ms a logo appears in
            // the region, 500 ms one antialiasing-level pixel flickers outside it, 600 ms text
            // reflows, and the browser-chrome row at the bottom changes in every frame.
            WriteFilmstrip(directory, new[]
            {
                Frame(dark: true),
                Frame(text: true),
                Frame(text: true),
                Frame(text: true, logo: true),
                Frame(text: true, logo: true, flicker: true),
                Frame(text: true, logo: true, reflow: true),
            });
            var page = PixelRegion.Parse("0,0,40,19");
            var settle = FilmstripSettleAnalysis.Analyze(directory, page, PixelRegion.Parse("30,0,10,10"));
            Check(settle is { Frames: 6, CapturedToMs: 600, FirstContentMs: 200, RegionSettledMs: 400,
                LastChangeMs: 600, Content: "0,0,40,19", Region: "30,0,10,10" },
                $"unexpected settle report {settle}");
            var withoutRegion = FilmstripSettleAnalysis.Analyze(directory, page, null);
            Check(withoutRegion is { RegionSettledMs: null, Region: null }, "a region was reported unasked");
            var blank = FilmstripSettleAnalysis.Analyze(directory, page, PixelRegion.Parse("0,14,5,5"));
            Check(blank.RegionSettledMs is null, "a region that never shows content cannot settle");
            var whole = FilmstripSettleAnalysis.Analyze(directory, null, null);
            Check(whole is { FirstContentMs: 100, LastChangeMs: 600 },
                $"browser chrome must count when no content area is given: {whole}");
            using var encoded = JsonDocument.Parse(JsonSerializer.Serialize(settle, JsonDefaults.Options));
            Check(encoded.RootElement.GetProperty("last_change_ms").GetDouble() == 600,
                "settle JSON lost its snake_case fields");
            Console.WriteLine("Filmstrip settle analysis tests passed without browser execution.");
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    private static int frames;

    private static byte[] Frame(bool dark = false, bool text = false, bool logo = false,
        bool flicker = false, bool reflow = false)
    {
        var rgb = Enumerable.Repeat(dark ? (byte)20 : (byte)255, Width * Height * 3).ToArray();
        void Paint(int x, int y, byte value)
        {
            var offset = (y * Width + x) * 3;
            rgb[offset] = rgb[offset + 1] = rgb[offset + 2] = value;
        }
        if (text)
            for (var x = reflow ? 4 : 2; x < 20; x++) Paint(x, 12, 0);
        if (logo)
            for (var y = 2; y < 8; y++) for (var x = 32; x < 38; x++) Paint(x, y, 40);
        if (flicker) Paint(0, 0, 245);
        // A browser-chrome row outside the content area, different in every frame.
        for (var x = 0; x <= frames % Width; x++) Paint(x, Height - 1, 90);
        frames++;
        return rgb;
    }

    private static void WriteFilmstrip(string directory, byte[][] frames)
    {
        var entries = new List<object>();
        for (var index = 0; index < frames.Length; index++)
        {
            var file = $"frame-{(index + 1) * 100:000000}ms.png";
            File.WriteAllBytes(Path.Combine(directory, file), EncodePng(frames[index]));
            entries.Add(new { scheduled_ms = (index + 1) * 100, captured_ms = (index + 1) * 100.0, file,
                error = (string?)null });
        }
        // A failed capture is skipped rather than compared.
        entries.Add(new { scheduled_ms = 700, captured_ms = 700.0, file = "frame-000700ms.png",
            error = "capture failed" });
        File.WriteAllText(Path.Combine(directory, "manifest.json"),
            JsonSerializer.Serialize(new { anchor = "navigation_start", frames = entries }));
    }

    /// <summary>An RGB PNG with unfiltered scanlines; the decoder under test ignores CRCs.</summary>
    private static byte[] EncodePng(byte[] rgb)
    {
        using var output = new MemoryStream();
        output.Write(new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 });
        var header = new byte[13];
        BinaryPrimitives.WriteInt32BigEndian(header, Width);
        BinaryPrimitives.WriteInt32BigEndian(header.AsSpan(4), Height);
        header[8] = 8;
        header[9] = 2;
        Chunk(output, "IHDR", header);
        using var compressed = new MemoryStream();
        using (var deflater = new ZLibStream(compressed, CompressionLevel.Fastest, leaveOpen: true))
        {
            for (var y = 0; y < Height; y++)
            {
                deflater.WriteByte(0);
                deflater.Write(rgb, y * Width * 3, Width * 3);
            }
        }
        Chunk(output, "IDAT", compressed.ToArray());
        Chunk(output, "IEND", Array.Empty<byte>());
        return output.ToArray();
    }

    private static void Chunk(Stream output, string type, byte[] data)
    {
        var length = new byte[4];
        BinaryPrimitives.WriteInt32BigEndian(length, data.Length);
        output.Write(length);
        output.Write(System.Text.Encoding.ASCII.GetBytes(type));
        output.Write(data);
        output.Write(new byte[4]);
    }

    private static void Check(bool success, string message)
    {
        if (!success) throw new InvalidOperationException(message);
    }
}
