# Ordered action tokens reuse the browser's validated hidden CLI, never a second launcher.
function Get-HiddenBenchmarkActionArguments {
    param([AllowEmptyCollection()][string[]] $ActionSequence = @())
    $result = [System.Collections.Generic.List[string]]::new()
    foreach ($action in $ActionSequence) {
        if ([string]::IsNullOrWhiteSpace($action) -or $action -notmatch '^([a-z]+):(.*)$') {
            throw '-ActionSequence values require move:x,y, wheel:x,y,delta, scroll:y, or pause:milliseconds.'
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
            default { throw "Unsupported ordered hidden action: $kind" }
        }
        $result.Add($option)
        $result.Add($value)
    }
    $result.ToArray()
}
