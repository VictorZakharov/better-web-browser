# Filmstrip completion timing for page-load comparisons; dot-sourced by measure-page-load.ps1
# and its self-test.

# GDI+ decodes the filmstrip PNGs; .NET splits its types across private assemblies that the
# compiler must be given by path.
Add-Type -AssemblyName System.Drawing
if (-not ('FilmstripFrames' -as [type])) {
$drawing = @([AppDomain]::CurrentDomain.GetAssemblies() |
        Where-Object { $_.GetName().Name -match '^System\.(Drawing(\.Common|\.Primitives)?|Private\.Windows\.(GdiPlus|Core))$' } |
        ForEach-Object Location)
$runtime = Split-Path ([object].Assembly.Location)
$drawing += @('System.Private.Windows.Core.dll', 'System.Private.Windows.GdiPlus.dll') |
    ForEach-Object { Join-Path $runtime $_ } | Where-Object { (Test-Path $_) -and $drawing -notcontains $_ }
Add-Type -ReferencedAssemblies $drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class FilmstripFrames
{
    // Luma of every pixel in the rectangle, clipped to the image.
    public static byte[] Luma(string path, int x, int y, int width, int height)
    {
        using (var image = new Bitmap(path))
        {
            int left = Math.Max(0, x), top = Math.Max(0, y);
            int right = Math.Min(image.Width, x + width), bottom = Math.Min(image.Height, y + height);
            if (right <= left || bottom <= top) return new byte[0];
            var rect = new Rectangle(left, top, right - left, bottom - top);
            var data = image.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            try
            {
                var row = new byte[data.Stride];
                var luma = new byte[rect.Width * rect.Height];
                for (int r = 0; r < rect.Height; r++)
                {
                    Marshal.Copy(data.Scan0 + r * data.Stride, row, 0, data.Stride);
                    for (int c = 0; c < rect.Width; c++)
                    {
                        int i = c * 4;
                        luma[r * rect.Width + c] = (byte)((row[i] * 29 + row[i + 1] * 150 + row[i + 2] * 77) >> 8);
                    }
                }
                return luma;
            }
            finally { image.UnlockBits(data); }
        }
    }

    // Share of pixels whose luma differs by more than a small antialiasing tolerance.
    public static double Difference(byte[] a, byte[] b)
    {
        if (a.Length == 0 || a.Length != b.Length) return 1.0;
        int changed = 0;
        for (int i = 0; i < a.Length; i++) if (Math.Abs(a[i] - b[i]) > 24) changed++;
        return (double)changed / a.Length;
    }
}
'@
}

function Get-CompletionMs([object[]] $Frames, [int[]] $Rectangle, [double] $Tolerance) {
    # The first frame after which every frame matches the final one, so a region that flashes
    # its final pixels and then changes again counts from when it settles.
    $luma = @($Frames | ForEach-Object { , [FilmstripFrames]::Luma($_.FullName, $Rectangle[0], $Rectangle[1], $Rectangle[2], $Rectangle[3]) })
    $final = $luma[-1]
    $complete = $Frames.Count - 1
    for ($index = $Frames.Count - 1; $index -ge 0; $index--) {
        if ([FilmstripFrames]::Difference($luma[$index], $final) -gt $Tolerance) { break }
        $complete = $index
    }
    [int]($Frames[$complete].BaseName -replace '\D', '')
}
