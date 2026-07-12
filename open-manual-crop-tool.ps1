param(
  [string]$Images = "",
  [string]$Output = "",
  [string]$Boxes = "",
  [int]$Port = 8765,
  [string[]]$Pattern = @(),
  [switch]$Recursive
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not $Images) {
  $DefaultT2In = "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\checkin-counter-full-crop\output\zone-crops-wide"
  if (Test-Path -LiteralPath $DefaultT2In) {
    $Images = $DefaultT2In
    if ($Pattern.Count -eq 0) {
      $Pattern = @("ck-*-wide-crop.png")
    }
  } else {
    $Images = Join-Path $Root "refs"
  }
}

if (-not $Output) {
  $Output = Join-Path $Images "manual-crops"
}

if (-not $Boxes) {
  $Boxes = Join-Path $Output "manual-crop-boxes.json"
}

$ArgsList = @(
  (Join-Path $Root "tools\manual_crop_tool.py"),
  "--images", $Images,
  "--output", $Output,
  "--boxes", $Boxes,
  "--port", "$Port",
  "--open"
)

if ($Recursive) {
  $ArgsList += "--recursive"
}

foreach ($Item in $Pattern) {
  $ArgsList += @("--pattern", $Item)
}

Write-Host "Manual Crop Tool"
Write-Host "Images: $Images"
Write-Host "Output: $Output"
Write-Host "Boxes : $Boxes"
Write-Host "URL   : http://127.0.0.1:$Port/"
Write-Host ""
Write-Host "Press Ctrl+C in this window to stop the tool."

python @ArgsList
