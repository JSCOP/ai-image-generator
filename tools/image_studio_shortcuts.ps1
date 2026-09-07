param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('Desktop', 'Startup', 'RemoveStartup')]
  [string]$ShortcutAction,

  [ValidateRange(1, 65535)]
  [int]$Port = 8766
)

# Owns this clone's Desktop/Startup shortcuts. Called by
# open-image-studio.ps1; not a public entry point on its own.
# Current-user folders only; never system-wide registration.
# Works on Windows PowerShell 5.1 and PowerShell 7.

$ErrorActionPreference = 'Stop'
$ToolsDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ToolsDir
Set-Location -LiteralPath $Root

function Fail([string]$Message) {
  Write-Host ''
  Write-Host ('오류: ' + $Message) -ForegroundColor Red
  exit 1
}

function Get-FullRoot {
  $P = [IO.Path]::GetFullPath($Root)
  return $P.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Get-CloneId {
  $Norm = (Get-FullRoot).ToLowerInvariant()
  $Sha = [Security.Cryptography.SHA256]::Create()
  try {
    $Hash = $Sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Norm))
  } finally {
    $Sha.Dispose()
  }
  return (($Hash[0..3] | ForEach-Object { $_.ToString('X2') }) -join '')
}

function Get-LinkName {
  $Leaf = Split-Path -Leaf (Get-FullRoot)
  $Safe = ('' + $Leaf -replace '[^\w\-]+', '-').Trim('-')
  if (-not $Safe) {
    $Safe = 'studio'
  }
  if ($Safe.Length -gt 24) {
    $Safe = $Safe.Substring(0, 24)
  }
  return ('AI Image Studio - {0}-{1}.lnk' -f $Safe, (Get-CloneId))
}

function Get-Shell {
  return (New-Object -ComObject 'WScript.Shell')
}

function Release-Shell($Shell) {
  try {
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($Shell)
  } catch {
  }
}

function New-Link([string]$Path, [string]$Target, [string]$Arguments, [string]$WorkDir, [int]$WindowStyle, [string]$Description) {
  $Shell = Get-Shell
  try {
    $Link = $Shell.CreateShortcut($Path)
    $Link.TargetPath = $Target
    $Link.Arguments = $Arguments
    $Link.WorkingDirectory = $WorkDir
    $Link.WindowStyle = $WindowStyle
    $Link.Description = $Description
    $Link.Save()
  } finally {
    Release-Shell $Shell
  }
}

function Normalize-LinkPath([string]$P) {
  if (-not $P) {
    return ''
  }
  $F = ('' + $P).Trim().TrimEnd('\', '/')
  return (($F -replace '/', '\').ToLowerInvariant())
}

function Get-ExpectedDesktopTarget {
  return (Join-Path (Get-FullRoot) 'open-image-studio.cmd')
}

function Get-ExpectedPowerShell {
  return (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe')
}

function Get-ExpectedPs1 {
  return (Join-Path (Get-FullRoot) 'open-image-studio.ps1')
}

# Owned ONLY on exact match: the canonical target plus this clone's
# working directory (Desktop shape), or powershell.exe with the exact
# quoted -File launcher path plus this clone's working directory
# (Startup shape). No substring matching: an adjacent clone path prefix
# or a stray argument mentioning this root never counts as owned.
function Test-OwnedLink([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) {
    return $false
  }
  $NormRoot = Normalize-LinkPath (Get-FullRoot)
  $Shell = Get-Shell
  try {
    $Link = $Shell.CreateShortcut($Path)
    $T = Normalize-LinkPath ('' + $Link.TargetPath)
    $A = '' + $Link.Arguments
    $W = Normalize-LinkPath ('' + $Link.WorkingDirectory)
    if ($W -ne $NormRoot) {
      return $false
    }
    if ($T -eq (Normalize-LinkPath (Get-ExpectedDesktopTarget))) {
      return $true
    }
    if ($T -eq (Normalize-LinkPath (Get-ExpectedPowerShell))) {
      $Expected = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + (Get-ExpectedPs1) + '" -NoOpen -Port '
      if ($A -match ('^' + [regex]::Escape($Expected) + '[1-9][0-9]{0,4}$')) {
        return $true
      }
    }
    return $false
  } catch {
    return $false
  } finally {
    Release-Shell $Shell
  }
}

$LinkName = Get-LinkName
$FullRoot = Get-FullRoot

if ($ShortcutAction -eq 'Desktop') {
  $Dir = [Environment]::GetFolderPath('Desktop')
  if (-not $Dir) {
    Fail('바탕화면 폴더를 찾을 수 없습니다 (Desktop folder not found).')
  }
  $Target = Join-Path $FullRoot 'open-image-studio.cmd'
  if (-not (Test-Path -LiteralPath $Target)) {
    Fail('런처를 찾을 수 없습니다 (launcher missing): ' + $Target)
  }
  $LinkArgs = ''
  if ($Port -ne 8766) {
    $LinkArgs = '-Port ' + $Port
  }
  $Dest = Join-Path $Dir $LinkName
  if ((Test-Path -LiteralPath $Dest) -and (-not (Test-OwnedLink $Dest))) {
    Fail('같은 이름의 바로가기가 있지만 이 클론의 것이 아니라 덮어쓰지 않았습니다 (foreign link left untouched): ' + $Dest)
  }
  New-Link $Dest $Target $LinkArgs $FullRoot 1 ('AI 이미지 스튜디오 바로가기 (this clone only): ' + $FullRoot)
  Write-Host ('바탕화면에 바로가기를 만들었습니다: ' + $Dest)
  exit 0
}

if ($ShortcutAction -eq 'Startup') {
  $Dir = [Environment]::GetFolderPath('Startup')
  if (-not $Dir) {
    Fail('시작프로그램 폴더를 찾을 수 없습니다 (Startup folder not found).')
  }
  $Ps1 = Join-Path $FullRoot 'open-image-studio.ps1'
  if (-not (Test-Path -LiteralPath $Ps1)) {
    Fail('런처를 찾을 수 없습니다 (launcher missing): ' + $Ps1)
  }
  $PowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  $LinkArgs = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $Ps1 + '" -NoOpen -Port ' + $Port
  $Dest = Join-Path $Dir $LinkName
  if ((Test-Path -LiteralPath $Dest) -and (-not (Test-OwnedLink $Dest))) {
    Fail('같은 이름의 바로가기가 있지만 이 클론의 것이 아니라 덮어쓰지 않았습니다 (foreign link left untouched): ' + $Dest)
  }
  New-Link $Dest $PowerShell $LinkArgs $FullRoot 7 ('AI 이미지 스튜디오 자동 시작 (hidden, this clone only): ' + $FullRoot)
  Write-Host ('로그인 자동 시작 바로가기를 만들었습니다 (현재 사용자): ' + $Dest)
  Write-Host '시작 시 브라우저 없이 숨은 창으로 서버가 실행됩니다 (-NoOpen).'
  exit 0
}

# RemoveStartup: delete ONLY this clone's verified Startup link.
$Dir = [Environment]::GetFolderPath('Startup')
if (-not $Dir) {
  Fail('시작프로그램 폴더를 찾을 수 없습니다 (Startup folder not found).')
}
$Dest = Join-Path $Dir $LinkName
if (Test-Path -LiteralPath $Dest) {
  if (-not (Test-OwnedLink $Dest)) {
    Fail('같은 이름의 바로가기가 있지만 이 클론의 것이 아니라 삭제하지 않았습니다 (foreign link left untouched): ' + $Dest)
  }
  Remove-Item -LiteralPath $Dest -Force
  Write-Host ('자동 시작 바로가기를 삭제했습니다: ' + $Dest)
  exit 0
}
Write-Host '삭제할 자동 시작 바로가기가 없습니다 (nothing to remove).'
exit 0
