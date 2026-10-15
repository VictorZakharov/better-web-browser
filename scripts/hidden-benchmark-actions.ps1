# Ordered action tokens reuse the browser's validated hidden CLI, never a second launcher.
function Get-HiddenBenchmarkActionArguments {
    param([AllowEmptyCollection()][string[]] $ActionSequence = @())
    $result = [System.Collections.Generic.List[string]]::new()
    foreach ($action in $ActionSequence) {
        if ([string]::IsNullOrWhiteSpace($action) -or $action -notmatch '^([a-z]+):(.*)$') {
            throw '-ActionSequence values require move:x,y, wheel:x,y,delta, scroll:y, pause:milliseconds, key/keydown/keyup:key,code, or activate:selector.'
        }
        $kind, $value = $Matches[1], $Matches[2]
        $option = switch ($kind) {
            'move' {
                if ($value -notmatch '^\d+\s*,\s*\d+$') {
                    throw 'Ordered move requires non-negative integer CSS document x,y coordinates.'
                }
                foreach ($coordinate in $value.Split(',')) {
                    $number = 0
                    if (-not [int]::TryParse($coordinate.Trim(), [ref] $number)) {
                        throw 'Ordered move coordinates must fit signed 32-bit integers.'
                    }
                }
                '--move-after-ready'
            }
            'wheel' {
                if ($value -notmatch '^\d+\s*,\s*\d+\s*,\s*-?\d+$') {
                    throw 'Ordered wheel requires x,y,delta CSS viewport/pixel coordinates.'
                }
                $limits = @(7680, 4320, 10000)
                $parts = $value.Split(',')
                for ($index = 0; $index -lt 3; $index++) {
                    $number = 0
                    if (-not [int]::TryParse($parts[$index].Trim(), [ref] $number) -or
                        [math]::Abs([long] $number) -gt $limits[$index]) {
                        throw 'Ordered wheel requires bounded x<=7680, y<=4320, abs(delta)<=10000.'
                    }
                }
                '--wheel-after-ready'
            }
            'scroll' {
                $number = 0
                if ($value -notmatch '^\d+$' -or -not [int]::TryParse($value, [ref] $number)) {
                    throw 'Ordered scroll requires integer CSS y from 0 to 2147483647.'
                }
                '--scroll-after-ready'
            }
            'pause' {
                $number = 0
                if ($value -notmatch '^\d+$' -or -not [int]::TryParse($value, [ref] $number) -or $number -gt 60000) {
                    throw 'Ordered pause requires integer milliseconds from 0 to 60000.'
                }
                '--pause-after-ready'
            }
            { $_ -in @('key','keydown','keyup') } {
                if ($value -notmatch '^[^,]+,[^,]+$' -or $value.Length -gt 256 -or $value.Contains([char]0)) {
                    throw 'Ordered key requires a bounded key,code pair without NUL.'
                }
                switch ($kind) {
                    'keydown' { '--key-down-after-ready' }
                    'keyup' { '--key-up-after-ready' }
                    default { '--key-after-ready' }
                }
            }
            'activate' {
                if ([string]::IsNullOrWhiteSpace($value) -or $value.Length -gt 4096 -or $value.Contains([char]0)) {
                    throw 'Ordered activation requires a non-empty bounded selector without NUL.'
                }
                '--activate-selector-after-ready'
            }
            'browser' {
                $resize = $value -match '^resize=(\d+)x(\d+)$'
                if ($resize) {
                    $width = 0; $height = 0
                    if (-not [int]::TryParse($Matches[1], [ref] $width) -or -not [int]::TryParse($Matches[2], [ref] $height) -or
                        $width -lt 320 -or $width -gt 7680 -or $height -lt 240 -or $height -gt 4320) {
                        throw 'Ordered native resize requires WIDTHxHEIGHT within 320..7680 by 240..4320 DIP.'
                    }
                } elseif ($value -notin @('new-tab','next-tab','previous-tab','close-tab','reader','fullscreen')) {
                    throw 'Ordered browser action requires a tab action, reader, fullscreen or resize=WIDTHxHEIGHT.'
                }
                '--browser-action-after-ready'
            }
            default { throw "Unsupported ordered hidden action: $kind" }
        }
        $result.Add($option)
        $result.Add($value)
    }
    $result.ToArray()
}
