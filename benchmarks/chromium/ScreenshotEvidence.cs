using System.Buffers.Binary;
using System.IO.Compression;

namespace ChromiumBaseline;

internal readonly record struct ScreenshotEvidence(int DistinctColors, double PaintedPixelRatio)
{
    private const int PngSignatureLength = 8;

    public static ScreenshotEvidence Analyze(byte[] png)
    {
        if (png.Length < PngSignatureLength ||
            !png.AsSpan(0, PngSignatureLength).SequenceEqual(new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 }))
        {
            throw new InvalidDataException("Chromium screenshot was not a PNG image.");
        }

        var width = 0;
        var height = 0;
        var bitDepth = 0;
        var colorType = 0;
        var interlace = 0;
        using var compressed = new MemoryStream();
        for (var offset = PngSignatureLength; offset + 12 <= png.Length;)
        {
            var length = BinaryPrimitives.ReadInt32BigEndian(png.AsSpan(offset, 4));
            if (length < 0 || offset + 12L + length > png.Length)
            {
                throw new InvalidDataException("Chromium screenshot contained an invalid PNG chunk.");
            }
            var type = png.AsSpan(offset + 4, 4);
            var data = png.AsSpan(offset + 8, length);
            if (type.SequenceEqual("IHDR"u8))
            {
                width = BinaryPrimitives.ReadInt32BigEndian(data[..4]);
                height = BinaryPrimitives.ReadInt32BigEndian(data.Slice(4, 4));
                bitDepth = data[8];
                colorType = data[9];
                interlace = data[12];
            }
            else if (type.SequenceEqual("IDAT"u8))
            {
                compressed.Write(data);
            }
            else if (type.SequenceEqual("IEND"u8))
            {
                break;
            }
            offset += 12 + length;
        }

        var channels = colorType switch
        {
            0 => 1,
            2 => 3,
            4 => 2,
            6 => 4,
            _ => throw new InvalidDataException($"Unsupported Chromium screenshot PNG color type {colorType}.")
        };
        if (width <= 0 || height <= 0 || bitDepth != 8 || interlace != 0)
        {
            throw new InvalidDataException("Chromium screenshot used an unsupported PNG encoding.");
        }

        var stride = checked(width * channels);
        var encoded = new byte[checked((stride + 1) * height)];
        compressed.Position = 0;
        using (var inflater = new ZLibStream(compressed, CompressionMode.Decompress, leaveOpen: true))
        {
            inflater.ReadExactly(encoded);
            if (inflater.ReadByte() != -1)
            {
                throw new InvalidDataException("Chromium screenshot PNG contained unexpected scanline data.");
            }
        }

        var previous = new byte[stride];
        var current = new byte[stride];
        var colors = new bool[32 * 32 * 32];
        var distinct = 0;
        long painted = 0;
        var source = 0;
        for (var y = 0; y < height; y++)
        {
            var filter = encoded[source++];
            for (var x = 0; x < stride; x++)
            {
                var left = x >= channels ? current[x - channels] : 0;
                var above = previous[x];
                var aboveLeft = x >= channels ? previous[x - channels] : 0;
                var predictor = filter switch
                {
                    0 => 0,
                    1 => left,
                    2 => above,
                    3 => (left + above) / 2,
                    4 => Paeth(left, above, aboveLeft),
                    _ => throw new InvalidDataException($"Chromium screenshot PNG used unknown filter {filter}.")
                };
                current[x] = unchecked((byte)(encoded[source++] + predictor));
            }

            for (var x = 0; x < width; x++)
            {
                var pixel = x * channels;
                var (red, green, blue, alpha) = colorType switch
                {
                    0 => (current[pixel], current[pixel], current[pixel], (byte)255),
                    2 => (current[pixel], current[pixel + 1], current[pixel + 2], (byte)255),
                    4 => (current[pixel], current[pixel], current[pixel], current[pixel + 1]),
                    6 => (current[pixel], current[pixel + 1], current[pixel + 2], current[pixel + 3]),
                    _ => throw new InvalidDataException($"Unsupported Chromium screenshot PNG color type {colorType}.")
                };
                red = CompositeOverWhite(red, alpha);
                green = CompositeOverWhite(green, alpha);
                blue = CompositeOverWhite(blue, alpha);
                var bucket = (red >> 3) << 10 | (green >> 3) << 5 | blue >> 3;
                if (!colors[bucket])
                {
                    colors[bucket] = true;
                    distinct++;
                }
                if (255 - red > 12 || 255 - green > 12 || 255 - blue > 12)
                {
                    painted++;
                }
            }
            (previous, current) = (current, previous);
        }

        return new ScreenshotEvidence(distinct, painted / (double)(width * height));
    }

    private static byte CompositeOverWhite(byte value, byte alpha) =>
        (byte)((value * alpha + 255 * (255 - alpha) + 127) / 255);

    private static int Paeth(int left, int above, int aboveLeft)
    {
        var prediction = left + above - aboveLeft;
        var leftDistance = Math.Abs(prediction - left);
        var aboveDistance = Math.Abs(prediction - above);
        var diagonalDistance = Math.Abs(prediction - aboveLeft);
        return leftDistance <= aboveDistance && leftDistance <= diagonalDistance
            ? left
            : aboveDistance <= diagonalDistance ? above : aboveLeft;
    }
}
