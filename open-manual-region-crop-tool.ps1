param(
  [string]$Images = "D:\Eagle\CityAI.library\images\MQEVG3G8GDG8U.info\Clipboard - 2026-06-15 16.08.16.png",
  [string]$Output = "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\checkin-counter-manual-region-crops-v1\output\manual-region-crops",
  [string]$Boxes = "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\checkin-counter-manual-region-crops-v1\manual-region-crop-boxes.json",
  [string]$Regions = "A,B,C,D,E,F,G,H,J,K,L,M,N",
  [int]$Port = 8766,
  [string[]]$Pattern = @(),
  [switch]$Recursive
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

$ArgsList = @(
  (Join-Path $Root "tools\manual_region_crop_tool.py"),
  "--images", $Images,
  "--output", $Output,
  "--boxes", $Boxes,
  "--regions", $Regions,
  "--port", "$Port",
  "--open"
)

foreach ($Item in $Pattern) {
  $ArgsList += @("--pattern", $Item)
}

if ($Recursive) {
  $ArgsList += "--recursive"
}

Write-Host "Manual Region Crop Tool"
Write-Host "Images : $Images"
Write-Host "Regions: $Regions"
Write-Host "Output : $Output"
Write-Host "Boxes  : $Boxes"
Write-Host "URL    : http://127.0.0.1:$Port/"
Write-Host ""
Write-Host "Press Ctrl+C in this window to stop the tool."

python @ArgsList
