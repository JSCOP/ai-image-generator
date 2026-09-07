param(
  [ValidateRange(1, 65535)]
  [int]$Port = 8766,

  [switch]$NoOpen,

  [switch]$Stop,

  [ValidateSet('', 'Desktop', 'Startup', 'RemoveStartup')]
  [string]$ShortcutAction = ''
)

# AI Image Studio one-click launcher (this clone only).
# Works on Windows PowerShell 5.1 and PowerShell 7.
# Uses the clone-local .venv (created on demand) and runs
# tools/image_studio.py from the worktree root. Existing-instance
# reuse is handled inside the server CLI, not here.

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location -LiteralPath $Root

function Fail([string]$Message) {
  Write-Host ''
  Write-Host ('오류: ' + $Message) -ForegroundColor Red
  exit 1
}

function Normalize-LocalRoot([string]$P) {
  if (-not $P) {
    return ''
  }
  $F = [IO.Path]::GetFullPath($P).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
  return (($F -replace '/', '\').ToLowerInvariant())
}

function Get-HttpStatusCode($Err) {
  try {
    if ($Err.Exception.Response -and $Err.Exception.Response.StatusCode) {
      return [int]$Err.Exception.Response.StatusCode
    }
  } catch {
  }
  try {
    if ($Err.Exception.StatusCode) {
      return [int]$Err.Exception.StatusCode
    }
  } catch {
  }
  return $null
}

# Stops ONLY this clone's verified server on $Port. Pure HTTP, no Python.
# Exit 0 when stopped or already stopped; Fail (exit 1) on any refusal.
function Stop-OwnServer {
  $Base = 'http://127.0.0.1:' + $Port
  $Health = $null
  $HttpCode = $null
  try {
    $Health = Invoke-RestMethod -UseBasicParsing -TimeoutSec 5 -Uri ($Base + '/api/health')
  } catch {
    $HttpCode = Get-HttpStatusCode $_
  }
  if ($HttpCode) {
    Fail('포트 ' + $Port + '에서 알 수 없는 서비스가 응답했습니다 (HTTP ' + $HttpCode + ', not our server). 중지하지 않았습니다.')
  }
  if (-not $Health) {
    Write-Host ('실행 중인 서버가 없습니다 (no running server on ' + $Base + ').')
    return
  }
  if (('' + $Health.app) -ne 'ai-image-studio') {
    Fail('포트 ' + $Port + '의 서버가 다른 앱입니다 (app=' + $Health.app + '). 이 클론의 서버가 아니므로 중지하지 않았습니다.')
  }
  if ((Normalize-LocalRoot ('' + $Health.root)) -ne (Normalize-LocalRoot $Root)) {
    Fail('포트 ' + $Port + '의 서버 루트가 이 클론과 다릅니다 (server root=' + $Health.root + '). 다른 클론이거나 이전 버전이므로 중지하지 않았습니다.')
  }
  $Token = ''
  try {
    $Config = Invoke-RestMethod -UseBasicParsing -TimeoutSec 5 -Uri ($Base + '/api/config')
    $Token = '' + $Config.csrf_token
  } catch {
    $Token = ''
  }
  if (-not $Token) {
    Fail('CSRF 토큰을 받지 못했습니다 (GET /api/config failed or missing csrf_token). 서버 버전을 확인하세요.')
  }
  try {
    $null = Invoke-RestMethod -UseBasicParsing -TimeoutSec 10 -Method Post -Uri ($Base + '/api/shutdown') -Headers @{'Origin' = $Base; 'X-Studio-Token' = $Token} -Body '{}' -ContentType 'application/json'
  } catch {
    $Code = Get-HttpStatusCode $_
    if ($Code -eq 409) {
      Fail('실행 중인 작업이 있어 서버가 종료를 거부했습니다 (409: job active). 작업 완료 후 다시 -Stop 하세요.')
    }
    if ($Code) {
      Fail('종료 요청 실패 (shutdown failed, HTTP ' + $Code + '): ' + $_.Exception.Message)
    }
    Fail('종료 요청 실패 (shutdown request failed): ' + $_.Exception.Message)
  }
  Write-Host ('서버를 종료했습니다 (server stopped): ' + $Base)
}

# -Stop never touches Python/venv and never starts a server.
if ($Stop) {
  if ($ShortcutAction -ne '') {
    Fail('-Stop과 -ShortcutAction은 함께 사용할 수 없습니다 (mutually exclusive).')
  }
  Stop-OwnServer
  exit 0
}

# One-shot shortcut operations never start the server.
if ($ShortcutAction -ne '') {
  $Helper = Join-Path $Root 'tools\image_studio_shortcuts.ps1'
  if (-not (Test-Path -LiteralPath $Helper)) {
    Fail('바로가기 도우미를 찾을 수 없습니다 (shortcut helper missing): ' + $Helper)
  }
  try {
    & $Helper -ShortcutAction $ShortcutAction -Port $Port
    exit $LASTEXITCODE
  } catch {
    Fail('바로가기 작업 실패 (shortcut operation failed): ' + $_.Exception.Message)
  }
}

$Server = Join-Path $Root 'tools\image_studio.py'
if (-not (Test-Path -LiteralPath $Server)) {
  Fail('서버 스크립트가 없습니다 (backend not integrated yet): tools\image_studio.py')
}

# Prefer a working "py -3" runtime >= 3.11, then a working "python".
# Anything missing, broken, or older than 3.11 is skipped, never used.
$Candidates = @(
  @{ Label = 'py -3'; Cmd = @('py', '-3') },
  @{ Label = 'python'; Cmd = @('python') }
)
$BaseExe = $null
$BasePre = @()
$Problems = @()
foreach ($C in $Candidates) {
  $Exe = $C.Cmd[0]
  $Pre = @()
  if ($C.Cmd.Count -gt 1) {
    $Pre = $C.Cmd[1..($C.Cmd.Count - 1)]
  }
  $Version = ''
  try {
    $Out = & $Exe @Pre -c 'import sys; print(sys.version.split()[0])' 2>$null
    if ($LASTEXITCODE -eq 0 -and $Out) {
      $Version = ('' + $Out).Trim()
    }
  } catch {
    $Version = ''
  }
  if (-not $Version) {
    $Problems += ($C.Label + ': 실행할 수 없음 (not runnable)')
    continue
  }
  $Parts = $Version.Split('.')
  $Major = 0
  $Minor = 0
  [void][int]::TryParse($Parts[0], [ref]$Major)
  if ($Parts.Count -gt 1) {
    [void][int]::TryParse($Parts[1], [ref]$Minor)
  }
  if ($Major -gt 3 -or ($Major -eq 3 -and $Minor -ge 11)) {
    $BaseExe = $Exe
    $BasePre = $Pre
    break
  }
  $Problems += ($C.Label + ': Python ' + $Version + ' (3.11 이상 필요, needs 3.11+)')
}
if (-not $BaseExe) {
  $Detail = $Problems -join '; '
  Fail('사용 가능한 Python 3.11+ 인터프리터가 없습니다 (no usable Python 3.11+; Python을 설치한 뒤 "py -3 --version"이 3.11 이상인지 확인하세요). ' + $Detail)
}

# Clone-local virtualenv; created when missing, never deleted by this script.
$VenvDir = Join-Path $Root '.venv'
$VenvPython = Join-Path $VenvDir 'Scripts\python.exe'
$VenvOk = $false
if (Test-Path -LiteralPath $VenvPython) {
  try {
    & $VenvPython -c 'import sys; sys.exit(0 if sys.version_info>=(3,11) else 1)' 2>$null
    if ($LASTEXITCODE -eq 0) {
      $VenvOk = $true
    }
  } catch {
    $VenvOk = $false
  }
}
if (-not $VenvOk) {
  if (Test-Path -LiteralPath $VenvDir) {
    Fail('기존 .venv를 사용할 수 없어 그대로 두었습니다 (existing .venv preserved, not deleted): ' + $VenvPython + '. .venv 폴더 이름을 바꾸거나 삭제한 뒤 다시 실행하세요 (rename/remove .venv and rerun).')
  }
  Write-Host '가상환경(.venv)을 만드는 중... (creating clone-local .venv)'
  try {
    & $BaseExe @BasePre -m venv $VenvDir
  } catch {
    Fail('가상환경 생성 실패 (venv creation failed): ' + $_.Exception.Message)
  }
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $VenvPython)) {
    Fail('가상환경 생성 실패 (venv creation failed, exit ' + $LASTEXITCODE + '). "' + $BaseExe + ' -m venv ' + $VenvDir + '" 를 직접 실행해 오류를 확인하세요.')
  }
}

# requirements.txt (Pillow pin) installs only when Pillow is missing
# or its major version is wrong; nothing else is ever installed.
$PillowOk = $false
try {
  & $VenvPython -c 'import PIL; import sys; sys.exit(0 if int(PIL.__version__.split(chr(46))[0])==12 else 1)' 2>$null
  if ($LASTEXITCODE -eq 0) {
    $PillowOk = $true
  }
} catch {
  $PillowOk = $false
}
if (-not $PillowOk) {
  $Req = Join-Path $Root 'requirements.txt'
  if (-not (Test-Path -LiteralPath $Req)) {
    Fail('requirements.txt이 없습니다 (missing): ' + $Req)
  }
  Write-Host '필요 패키지(Pillow)를 설치하는 중... (installing pinned Pillow)'
  try {
    & $VenvPython -m pip install -r $Req
  } catch {
    Fail('패키지 설치 실패 (pip install failed): ' + $_.Exception.Message)
  }
  if ($LASTEXITCODE -ne 0) {
    Fail('패키지 설치 실패 (pip install failed, exit ' + $LASTEXITCODE + '). "' + $VenvPython + ' -m pip install -r ' + $Req + '" 를 직접 실행해 오류를 확인하세요.')
  }
  try {
    & $VenvPython -c 'import PIL; import sys; sys.exit(0 if int(PIL.__version__.split(chr(46))[0])==12 else 1)' 2>$null
    if ($LASTEXITCODE -eq 0) {
      $PillowOk = $true
    }
  } catch {
    $PillowOk = $false
  }
  if (-not $PillowOk) {
    Fail('Pillow 확인 실패 (Pillow major version 12 required). pip 설치는 끝났지만 Pillow 12를 가져오지 못했습니다.')
  }
}

$ServerArgs = @($Server, '--port', "$Port")
if (-not $NoOpen) {
  $ServerArgs += '--open'
}

Write-Host 'AI 이미지 스튜디오 (AI Image Studio)'
Write-Host ("URL   : http://127.0.0.1:{0}/" -f $Port)
Write-Host '중지  : 이 창에서 Ctrl+C (stop: press Ctrl+C in this window)'
Write-Host ''

try {
  & $VenvPython @ServerArgs
} catch {
  Fail('서버 실행 실패 (server failed to run): ' + $_.Exception.Message)
}
exit $LASTEXITCODE
