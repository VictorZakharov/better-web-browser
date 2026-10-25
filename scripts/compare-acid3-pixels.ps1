# Compares the page area of two hidden-browser screenshots (Acid3 and its reference rendering, taken
# at the same window size) and writes a diff image with differing pixels in magenta over a faded copy
# of the actual rendering. Both pages paint the #c0c0c0 canvas at their left edge, which locates the
# page area below the toolbar and above the status bar without depending on the browser's chrome.
[CmdletBinding()]
param([Parameter(Mandatory)][string] $Actual, [Parameter(Mandatory)][string] $Reference, [Parameter(Mandatory)][string] $Diff)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
if (-not ('Acid3PixelDiff' -as [type])) {
    # System.Drawing's public types forward to several implementation assemblies in .NET 9+.
    [void][Drawing.Bitmap]::new(1, 1)
    $drawing = @([AppDomain]::CurrentDomain.GetAssemblies() |
        Where-Object { $_.GetName().Name -match '^System\.(Drawing|Private\.Windows)' } | ForEach-Object Location)
    Add-Type -ReferencedAssemblies ($drawing + 'System.Runtime.InteropServices') -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class Acid3PixelDiff {
    const int Canvas = unchecked((int)0xFFC0C0C0);

    static int[] Read(string path, out int width, out int height) {
        using (var bitmap = new Bitmap(path)) {
            width = bitmap.Width;
            height = bitmap.Height;
            var data = bitmap.LockBits(new Rectangle(0, 0, width, height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            var pixels = new int[width * height];
            Marshal.Copy(data.Scan0, pixels, 0, pixels.Length);
            bitmap.UnlockBits(data);
            return pixels;
        }
    }

    static (int top, int bottom) PageRows(int[] pixels, int width, int height) {
        int top = -1, bottom = -1;
        for (int y = 0; y < height; y++)
            if (pixels[y * width] == Canvas) { if (top < 0) top = y; bottom = y; }
        if (top < 0) throw new InvalidOperationException("No #c0c0c0 canvas found at the left edge of the screenshot.");
        return (top, bottom);
    }

    public static long[] Compare(string actual, string reference, string diff) {
        var a = Read(actual, out int width, out int height);
        var b = Read(reference, out int referenceWidth, out int referenceHeight);
        if (width != referenceWidth || height != referenceHeight)
            throw new InvalidOperationException($"Screenshot sizes differ: {width}x{height} and {referenceWidth}x{referenceHeight}.");
        var rows = PageRows(a, width, height);
        if (rows != PageRows(b, width, height))
            throw new InvalidOperationException("The page areas of the two screenshots differ; were they taken with the same window settings?");
        int pageHeight = rows.bottom - rows.top + 1;
        var output = new int[width * pageHeight];
        long different = 0;
        for (int y = 0; y < pageHeight; y++) {
            for (int x = 0; x < width; x++) {
                int pixel = a[(y + rows.top) * width + x];
                if (pixel == b[(y + rows.top) * width + x]) {
                    int gray = (((pixel >> 16) & 255) + ((pixel >> 8) & 255) + (pixel & 255)) / 3;
                    int light = 192 + gray / 4;
                    output[y * width + x] = unchecked((int)0xFF000000) | (light << 16) | (light << 8) | light;
                } else {
                    different++;
                    output[y * width + x] = unchecked((int)0xFFFF00FF);
                }
            }
        }
        using (var bitmap = new Bitmap(width, pageHeight, PixelFormat.Format32bppArgb)) {
            var data = bitmap.LockBits(new Rectangle(0, 0, width, pageHeight), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
            Marshal.Copy(output, 0, data.Scan0, output.Length);
            bitmap.UnlockBits(data);
            bitmap.Save(diff, ImageFormat.Png);
        }
        return new long[] { different, (long)width * pageHeight };
    }
}
'@
}
$result = [Acid3PixelDiff]::Compare([IO.Path]::GetFullPath($Actual), [IO.Path]::GetFullPath($Reference), [IO.Path]::GetFullPath($Diff))
[pscustomobject]@{ DifferentPixels = $result[0]; ComparedPixels = $result[1]; Diff = $Diff }
