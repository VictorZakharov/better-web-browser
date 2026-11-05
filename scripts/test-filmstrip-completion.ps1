# Self-test for filmstrip completion timing (no browser): a region is complete at the first
# frame after which every frame matches the final one, and a whole-page tolerance still sees a
# change confined to a small region.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'filmstrip-completion.ps1')

$directory = Join-Path ([IO.Path]::GetTempPath()) "filmstrip-completion-$PID"
New-Item -ItemType Directory -Force -Path $directory | Out-Null
try {
    # 200x100 frames; the 20x20 "panel" at (150, 40) is dark in frames 200 and 400 and from 600 on.
    $panelDark = @{ 100 = $false; 200 = $true; 300 = $false; 400 = $true; 500 = $false; 600 = $true; 700 = $true }
    foreach ($ms in $panelDark.Keys) {
        $bitmap = [Drawing.Bitmap]::new(200, 100)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $graphics.Clear([Drawing.Color]::White)
        if ($panelDark[$ms]) { $graphics.FillRectangle([Drawing.Brushes]::Black, 150, 40, 20, 20) }
        $graphics.Dispose()
        $bitmap.Save((Join-Path $directory ('frame-{0:D6}ms.png' -f $ms)), [Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
    }
    $frames = @(Get-ChildItem -LiteralPath $directory -Filter 'frame-*ms.png' | Sort-Object { [int]($_.BaseName -replace '\D', '') })
    $region = Get-CompletionMs $frames @(150, 40, 20, 20) 0.01
    if ($region -ne 600) { throw "Region completion should be 600 ms after the flashes; got $region." }
    $page = Get-CompletionMs $frames @(0, 0, 200, 100) 0.001
    if ($page -ne 600) { throw "Whole-page completion should see the 2% panel change at 600 ms; got $page." }
    $outside = Get-CompletionMs $frames @(0, 0, 100, 100) 0.01
    if ($outside -ne 100) { throw "A region that never changes should be complete at the first frame; got $outside." }
    'Filmstrip completion tests passed.'
} finally {
    Remove-Item -LiteralPath $directory -Recurse -Force -ErrorAction SilentlyContinue
}
