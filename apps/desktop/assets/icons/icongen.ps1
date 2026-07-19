# icongen.ps1 - generates pure-study app icon (woven cross) PNGs + multi-res ICO
# Uses GDI+ (System.Drawing). Re-runnable; writes into this script's folder.
# Geometry is defined in a 256x256 master space and matches pure-study.svg.

Add-Type -AssemblyName System.Drawing

$OutDir = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }

# ---- palette --------------------------------------------------------------
$paper    = [System.Drawing.Color]::FromArgb(255, 252, 249, 244)   # #fcf9f4
$gold     = [System.Drawing.Color]::FromArgb(255, 158, 125, 56)    # #9e7d38
$darkGold = [System.Drawing.Color]::FromArgb(255, 133, 106, 46)    # #856a2e
$border   = [System.Drawing.Color]::FromArgb(64,  158, 125, 56)    # gold @ 25%
$shadeCol = [System.Drawing.Color]::FromArgb(100, 133, 106, 46)    # soft weave shadow

# ---- helpers --------------------------------------------------------------
function New-RoundRect([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x,          $y,          $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y,          $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0,   90)
    $p.AddArc($x,          $y + $h - $d, $d, $d, 90,  90)
    $p.CloseFigure()
    return $p
}

# Draw the whole icon into a Graphics already scaled to 256-space.
function Draw-Icon([System.Drawing.Graphics]$g) {
    $g.SmoothingMode     = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode  = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode    = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality

    $bPaper    = New-Object System.Drawing.SolidBrush($paper)
    $bGold     = New-Object System.Drawing.SolidBrush($gold)
    $bShade    = New-Object System.Drawing.SolidBrush($shadeCol)
    $penBorder = New-Object System.Drawing.Pen($border, 2)
    $penEdge   = New-Object System.Drawing.Pen($darkGold, 1.1)

    # paper tile + faint gold hairline
    $tile = New-RoundRect 1 1 254 254 48
    $g.FillPath($bPaper, $tile)
    $g.DrawPath($penBorder, $tile)

    # four gold strands (rounded bands)
    $vL = New-RoundRect 106 44 17 168 8
    $vR = New-RoundRect 133 44 17 168 8
    $hT = New-RoundRect 60  74 136 17 8
    $hB = New-RoundRect 60  101 136 17 8
    foreach ($s in @($vL, $vR, $hT, $hB)) {
        $g.FillPath($bGold, $s)
        $g.DrawPath($penEdge, $s)   # thin inked edge (letterpress feel)
    }

    # soft weave shadow: a faint darker band on the UNDER strand just beyond
    # each tuck gap, so the over strand reads as casting a shadow.
    $shades = @(
        @(100,74,3,17),  @(126,74,3,17),      # TL horizontal ducks under
        @(133,68,17,3),  @(133,94,17,3),      # TR vertical ducks under
        @(106,95,17,3),  @(106,121,17,3),     # BL vertical ducks under
        @(127,101,3,17), @(153,101,3,17)      # BR horizontal ducks under
    )
    foreach ($s in $shades) {
        $g.FillRectangle($bShade, [single]$s[0], [single]$s[1], [single]$s[2], [single]$s[3])
    }

    # basket-weave tuck gaps (paper) carve the UNDER strand at each crossing
    $gaps = @(
        @(103,74,3,17),  @(123,74,3,17),      # TL: vertical over horizontal
        @(133,71,17,3),  @(133,91,17,3),      # TR: horizontal over vertical
        @(106,98,17,3),  @(106,118,17,3),     # BL: horizontal over vertical
        @(130,101,3,17), @(150,101,3,17)      # BR: vertical over horizontal
    )
    foreach ($gp in $gaps) {
        $g.FillRectangle($bPaper, [single]$gp[0], [single]$gp[1], [single]$gp[2], [single]$gp[3])
    }

    $bPaper.Dispose(); $bGold.Dispose(); $bShade.Dispose()
    $penBorder.Dispose(); $penEdge.Dispose()
    foreach ($s in @($tile, $vL, $vR, $hT, $hB)) { $s.Dispose() }
}

# Render one size, supersampled by $ss then downscaled for crisp AA.
function Render-Size([int]$size, [int]$ss) {
    $S = $size * $ss
    $big = New-Object System.Drawing.Bitmap($S, $S, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $bg  = [System.Drawing.Graphics]::FromImage($big)
    $bg.Clear([System.Drawing.Color]::Transparent)
    $bg.ScaleTransform([single]($S / 256.0), [single]($S / 256.0))
    Draw-Icon $bg
    $bg.Dispose()
    if ($ss -eq 1) { return $big }

    $out = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $og  = [System.Drawing.Graphics]::FromImage($out)
    $og.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $og.SmoothingMode     = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $og.PixelOffsetMode   = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $og.Clear([System.Drawing.Color]::Transparent)
    $og.DrawImage($big, (New-Object System.Drawing.Rectangle(0, 0, $size, $size)),
                  0, 0, $S, $S, [System.Drawing.GraphicsUnit]::Pixel)
    $og.Dispose(); $big.Dispose()
    return $out
}

# ---- generate PNGs --------------------------------------------------------
$sizes = 16, 24, 32, 48, 64, 128, 256
$ss = 4
foreach ($sz in $sizes) {
    $bmp = Render-Size $sz $ss
    $path = Join-Path $OutDir ("pure-study-{0}.png" -f $sz)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host ("wrote {0}  ({1} bytes)" -f $path, (Get-Item $path).Length)
}

# ---- build multi-res ICO (embedded PNG payloads) --------------------------
$icoPath = Join-Path $OutDir "pure-study.ico"
$entries = foreach ($sz in $sizes) {
    $p = Join-Path $OutDir ("pure-study-{0}.png" -f $sz)
    [pscustomobject]@{ Size = $sz; Bytes = [System.IO.File]::ReadAllBytes($p) }
}
$ms = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter($ms)
$bw.Write([UInt16]0)                 # reserved
$bw.Write([UInt16]1)                 # type = icon
$bw.Write([UInt16]$entries.Count)    # image count
$offset = 6 + 16 * $entries.Count
foreach ($e in $entries) {
    $dim = if ($e.Size -ge 256) { 0 } else { $e.Size }
    $bw.Write([byte]$dim)            # width
    $bw.Write([byte]$dim)            # height
    $bw.Write([byte]0)              # color count
    $bw.Write([byte]0)              # reserved
    $bw.Write([UInt16]1)           # planes
    $bw.Write([UInt16]32)          # bit count
    $bw.Write([UInt32]$e.Bytes.Length)
    $bw.Write([UInt32]$offset)
    $offset += $e.Bytes.Length
}
foreach ($e in $entries) { $bw.Write($e.Bytes) }
$bw.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $ms.ToArray())
$bw.Dispose(); $ms.Dispose()
Write-Host ("wrote {0}  ({1} bytes)" -f $icoPath, (Get-Item $icoPath).Length)
