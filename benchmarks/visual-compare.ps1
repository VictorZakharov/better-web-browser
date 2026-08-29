[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $BreezeScreenshot,
    [Parameter(Mandatory)] [string] $ChromiumScreenshot,
    [Parameter(Mandatory)] [string] $BreezeReport,
    [ValidateRange(0, 1)] [double] $MaximumDifference = 0.48,
    [ValidateRange(0, 1)] [double] $MinimumLuminanceDeviation = 0.025,
    [switch] $DiagnosticOnly
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$report = Get-Content -LiteralPath $BreezeReport -Raw | ConvertFrom-Json
$scale = [double] $report.device_scale_factor
$contentWidth = [int] [Math]::Round([double] $report.viewport_width_css_px * $scale)
$contentHeight = [int] [Math]::Round([double] $report.viewport_height_css_px * $scale)
$contentTop = [int] [Math]::Round(104 * $scale)
$sampleSize = 64

function New-SampleBitmap {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [Drawing.Rectangle] $SourceRectangle
    )

    $source = [Drawing.Bitmap]::FromFile((Resolve-Path -LiteralPath $Path).Path)
    try {
        if ($SourceRectangle.Right -gt $source.Width -or $SourceRectangle.Bottom -gt $source.Height) {
            throw "Capture crop $SourceRectangle exceeds $($source.Width)x$($source.Height): $Path"
        }
        $sample = [Drawing.Bitmap]::new($sampleSize, $sampleSize)
        $graphics = [Drawing.Graphics]::FromImage($sample)
        try {
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBilinear
            $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.DrawImage(
                $source,
                [Drawing.Rectangle]::new(0, 0, $sampleSize, $sampleSize),
                $SourceRectangle,
                [Drawing.GraphicsUnit]::Pixel)
        } finally {
            $graphics.Dispose()
        }
        return $sample
    } finally {
        $source.Dispose()
    }
}

function Get-LuminanceDeviation {
    param([Parameter(Mandatory)] [Drawing.Bitmap] $Bitmap)

    $values = [Collections.Generic.List[double]]::new()
    for ($y = 0; $y -lt $Bitmap.Height; $y++) {
        for ($x = 0; $x -lt $Bitmap.Width; $x++) {
            $pixel = $Bitmap.GetPixel($x, $y)
            $values.Add((0.2126 * $pixel.R + 0.7152 * $pixel.G + 0.0722 * $pixel.B) / 255)
        }
    }
    $mean = ($values | Measure-Object -Average).Average
    $variance = ($values | ForEach-Object { ($_ - $mean) * ($_ - $mean) } | Measure-Object -Average).Average
    return [Math]::Sqrt($variance)
}

$breeze = New-SampleBitmap -Path $BreezeScreenshot -SourceRectangle (
    [Drawing.Rectangle]::new(0, $contentTop, $contentWidth, $contentHeight))
$chromiumSource = [Drawing.Bitmap]::FromFile((Resolve-Path -LiteralPath $ChromiumScreenshot).Path)
try {
    $chromiumRectangle = [Drawing.Rectangle]::new(0, 0, $chromiumSource.Width, $chromiumSource.Height)
} finally {
    $chromiumSource.Dispose()
}
$chromium = New-SampleBitmap -Path $ChromiumScreenshot -SourceRectangle $chromiumRectangle
try {
    $totalDifference = 0.0
    for ($y = 0; $y -lt $sampleSize; $y++) {
        for ($x = 0; $x -lt $sampleSize; $x++) {
            $left = $breeze.GetPixel($x, $y)
            $right = $chromium.GetPixel($x, $y)
            $totalDifference += [Math]::Abs($left.R - $right.R)
            $totalDifference += [Math]::Abs($left.G - $right.G)
            $totalDifference += [Math]::Abs($left.B - $right.B)
        }
    }
    $difference = $totalDifference / ($sampleSize * $sampleSize * 3 * 255)
    $breezeDeviation = Get-LuminanceDeviation $breeze
    $chromiumDeviation = Get-LuminanceDeviation $chromium
    $result = [pscustomobject]@{
        perceptual_difference = [Math]::Round($difference, 6)
        maximum_difference = $MaximumDifference
        breeze_luminance_deviation = [Math]::Round($breezeDeviation, 6)
        chromium_luminance_deviation = [Math]::Round($chromiumDeviation, 6)
        minimum_luminance_deviation = $MinimumLuminanceDeviation
        sample_width = $sampleSize
        sample_height = $sampleSize
        passed = $difference -le $MaximumDifference -and
            $breezeDeviation -ge $MinimumLuminanceDeviation -and
            $chromiumDeviation -ge $MinimumLuminanceDeviation
    }
    if (-not $result.passed -and -not $DiagnosticOnly) {
        throw "Visual gate failed: difference $($result.perceptual_difference) / $MaximumDifference; luminance deviations $($result.breeze_luminance_deviation), $($result.chromium_luminance_deviation)."
    }
    return $result
} finally {
    $breeze.Dispose()
    $chromium.Dispose()
}
