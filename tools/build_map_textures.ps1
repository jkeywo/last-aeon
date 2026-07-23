<#! Build exact visual and click-ID texture pairs from the authored provinces.
The visual output keeps the generated surface art but draws every Voronoi
border from the game's latitude/longitude data.  ID output has flat RGB
regions only: no border, filtering, alpha, or anti-aliasing. #>

Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root 'assets\textures\source'
$output = Join-Path $root 'assets\textures\maps'
$content = Get-Content -Raw (Join-Path $root 'assets\content\system\provinces.rhai')
New-Item -ItemType Directory -Force $output | Out-Null

$matches = [regex]::Matches($content, 'id: "(?<id>[^"]+)", body: "(?<body>[^"]+)",\s*latitude_mdeg: (?<lat>-?\d+), longitude_mdeg: (?<lon>-?\d+)')
$all = @{}
foreach ($match in $matches) {
    $body = $match.Groups['body'].Value
    if (-not $all.ContainsKey($body)) { $all[$body] = @() }
    $all[$body] += [pscustomobject]@{
        id = $match.Groups['id'].Value; lat = [double]$match.Groups['lat'].Value / 1000; lon = [double]$match.Groups['lon'].Value / 1000
    }
}

function Build-TexturePair([string]$body, [string]$sourceName) {
    $provinces = $all[$body]
    $width, $height = 768, 384
    $sourceBitmap = [System.Drawing.Bitmap]::new((Join-Path $source $sourceName))
    $visual = [System.Drawing.Bitmap]::new($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($visual)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.DrawImage($sourceBitmap, 0, 0, $width, $height)
    $graphics.Dispose(); $sourceBitmap.Dispose()
    $ids = [System.Drawing.Bitmap]::new($width, $height)

    $centres = foreach ($province in $provinces) {
        $lat = $province.lat * [Math]::PI / 180; $lon = $province.lon * [Math]::PI / 180
        [pscustomobject]@{ x = [Math]::Cos($lat) * [Math]::Cos($lon); y = [Math]::Sin($lat); z = -[Math]::Cos($lat) * [Math]::Sin($lon) }
    }
    $owners = [int[]]::new($width * $height)
    for ($y = 0; $y -lt $height; $y++) {
        $lat = (90 - (($y + 0.5) / $height * 180)) * [Math]::PI / 180
        $sinLat = [Math]::Sin($lat); $cosLat = [Math]::Cos($lat)
        for ($x = 0; $x -lt $width; $x++) {
            $lon = ((($x + 0.5) / $width * 360) - 180) * [Math]::PI / 180
            $dx = $cosLat * [Math]::Cos($lon); $dy = $sinLat; $dz = -$cosLat * [Math]::Sin($lon)
            $best = 0; $bestDot = [double]::NegativeInfinity
            for ($i = 0; $i -lt $centres.Count; $i++) {
                $centre = $centres[$i]; $dot = $dx * $centre.x + $dy * $centre.y + $dz * $centre.z
                if ($dot -gt $bestDot) { $best, $bestDot = $i, $dot }
            }
            $owners[$y * $width + $x] = $best
            $value = $best + 1
            $ids.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(($value -shr 16) -band 255, ($value -shr 8) -band 255, $value -band 255))
        }
    }
    for ($y = 0; $y -lt $height; $y++) {
        for ($x = 0; $x -lt $width; $x++) {
            $here = $owners[$y * $width + $x]
            $neighbours = @($owners[$y * $width + (($x - 1 + $width) % $width)], $owners[$y * $width + (($x + 1) % $width)], $owners[[Math]::Max(0, $y - 1) * $width + $x], $owners[[Math]::Min($height - 1, $y + 1) * $width + $x])
            if ($neighbours | Where-Object { $_ -ne $here }) {
                $visual.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(7, 10, 13))
            }
        }
    }
    $visual.Save((Join-Path $output "${body}_surface.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $ids.Save((Join-Path $output "${body}_province_ids.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $visual.Dispose(); $ids.Dispose()

    $lookup = [ordered]@{ body = $body; size = @($width, $height); provinces = [ordered]@{} }
    for ($i = 0; $i -lt $provinces.Count; $i++) { $value = $i + 1; $lookup.provinces[$provinces[$i].id] = @((($value -shr 16) -band 255), (($value -shr 8) -band 255), ($value -band 255)) }
    $lookup | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $output "${body}_province_ids.json")
}

Build-TexturePair 'ashkarr' 'ashkarr_ecumenopolis.png'
Build-TexturePair 'vesk' 'vesk_volcanic.png'
