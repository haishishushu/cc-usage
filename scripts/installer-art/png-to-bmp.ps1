# PNG -> 24 位无 alpha BMP。
# NSIS / MUI2 只认未压缩的 24 位位图，32 位带 alpha 通道的 BMP 会渲染成黑块或花屏。
param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$Target,
  [Parameter(Mandatory = $true)][int]$Width,
  [Parameter(Mandatory = $true)][int]$Height
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$src = [System.Drawing.Image]::FromFile((Resolve-Path $Source).Path)
try {
  $bmp = New-Object System.Drawing.Bitmap($Width, $Height, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
  try {
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
      # 2 倍渲染后高质量降采样，保证圆角和小字不发毛
      $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
      $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
      $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
      $g.Clear([System.Drawing.Color]::White)
      $rect = New-Object System.Drawing.Rectangle(0, 0, $Width, $Height)
      $g.DrawImage($src, $rect)
    } finally { $g.Dispose() }
    $bmp.Save($Target, [System.Drawing.Imaging.ImageFormat]::Bmp)
  } finally { $bmp.Dispose() }
} finally { $src.Dispose() }

Write-Output "ok $Target"
