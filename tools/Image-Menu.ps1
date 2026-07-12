# Interactive image generation menu for ai-image-generator.
# Usage: pwsh tools\Image-Menu.ps1

$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$GenImage = Join-Path $ProjectRoot 'scripts\gen_image.py'
$GenBatch = Join-Path $ProjectRoot 'scripts\gen_batch.py'
$PresetDir = Join-Path $ProjectRoot 'presets'

function Convert-SafeSlug($value) {
    $raw = if ($null -eq $value) { '' } else { $value.Trim() }
    $raw = [regex]::Replace($raw, '[^\p{L}\p{Nd}_\s\.-]+', '_')
    $raw = [regex]::Replace($raw, '\s+', '_').Trim(' ', '.', '_', '-')
    if ([string]::IsNullOrWhiteSpace($raw)) { return 'image-request' }
    if ($raw.Length -gt 80) { return $raw.Substring(0, 80) }
    return $raw
}

function Write-Title($t) {
    Write-Host ""
    Write-Host "=== $t ===" -ForegroundColor Cyan
}

function Ensure-Env {
    if (-not $env:CLIPROXY_BASE_URL) { $env:CLIPROXY_BASE_URL = 'http://localhost:8317/v1' }
    while (-not $env:CLIPROXY_API_KEY) {
        Write-Host "CLIPROXY_API_KEY 환경변수가 비어 있습니다." -ForegroundColor Yellow
        $k = Read-Host "API 키 입력"
        if (-not [string]::IsNullOrWhiteSpace($k)) {
            $env:CLIPROXY_API_KEY = $k
        }
    }
}

function Read-Default($prompt, $default) {
    $v = Read-Host "$prompt [$default]"
    if ([string]::IsNullOrWhiteSpace($v)) { return $default }
    return $v
}

function Read-IntDefault($prompt, $default, $min = 1, $max = 999999) {
    while ($true) {
        $v = Read-Host "$prompt [$default]"
        if ([string]::IsNullOrWhiteSpace($v)) { return [int]$default }
        $parsed = 0
        if ([int]::TryParse($v, [ref]$parsed)) {
            if ($parsed -ge $min -and $parsed -le $max) { return $parsed }
        }
        Write-Host "  유효한 정수 ($min ~ $max)를 입력하세요." -ForegroundColor Yellow
    }
}

function Read-YesNo($prompt, $default = 'n') {
    $v = Read-Default $prompt $default
    return ($v -ieq 'y' -or $v -ieq 'yes')
}

function Read-Size {
    Write-Host ""
    Write-Host "사이즈:"
    Write-Host "  [1] 1024x1024  (정사각)"
    Write-Host "  [2] 1280x720   (16:9 작음)"
    Write-Host "  [3] 1920x1080  (16:9 Full HD — 기본)"
    Write-Host "  [4] 1080x1920  (9:16 세로)"
    Write-Host "  [5] 직접 입력"
    $c = Read-Default "선택" "3"
    switch ($c) {
        '1' { return '1024x1024' }
        '2' { return '1280x720' }
        '3' { return '1920x1080' }
        '4' { return '1080x1920' }
        '5' {
            while ($true) {
                $s = Read-Host "  WIDTHxHEIGHT"
                if ($s -match '^\s*(\d+)\s*x\s*(\d+)\s*$') {
                    $w = [int]$matches[1]; $h = [int]$matches[2]
                    if ($w -gt 0 -and $h -gt 0) { return "${w}x${h}" }
                    Write-Host "  양수 크기를 입력해야 합니다." -ForegroundColor Yellow
                } else {
                    Write-Host "  형식 예: 1920x1080" -ForegroundColor Yellow
                }
            }
        }
        default { return '1920x1080' }
    }
}

function Read-Quality {
    Write-Host ""
    Write-Host "퀄리티:  [1] low   [2] medium   [3] high (기본)"
    $c = Read-Default "선택" "3"
    switch ($c) {
        '1' { return 'low' }
        '2' { return 'medium' }
        default { return 'high' }
    }
}

function Read-MultilinePrompt {
    Write-Host ""
    Write-Host "프롬프트 입력 방식:"
    Write-Host "  [1] 직접 입력 (END 또는 빈 줄 두 번이면 종료)"
    Write-Host "  [2] 텍스트 파일 경로"
    $c = Read-Default "선택" "1"
    if ($c -eq '2') {
        while ($true) {
            $p = Read-Host "텍스트 파일 경로"
            $p = $p.Trim('"',"'",' ')
            if (Test-Path -LiteralPath $p) {
                return (Get-Content -Raw -Encoding UTF8 -LiteralPath $p).Trim()
            }
            Write-Host "  파일 없음: $p" -ForegroundColor Yellow
        }
    }
    Write-Host "프롬프트 (END 또는 빈 줄 두 번 = 종료):"
    $lines = New-Object System.Collections.Generic.List[string]
    $emptyCount = 0
    while ($true) {
        $line = Read-Host
        if ($line -eq 'END') { break }
        if ([string]::IsNullOrWhiteSpace($line)) {
            $emptyCount++
            if ($emptyCount -ge 2) { break }
            $lines.Add('')
        } else {
            $emptyCount = 0
            $lines.Add($line)
        }
    }
    return ($lines -join "`n").Trim()
}

function Read-References {
    Write-Host ""
    Write-Host "레퍼런스 이미지 (선택). 경로 입력 후 Enter, 빈 줄로 종료."
    Write-Host "  드래그&드롭의 따옴표는 자동 제거됩니다."
    $refs = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $p = Read-Host "  [ref$($refs.Count + 1)]"
        if ([string]::IsNullOrWhiteSpace($p)) { break }
        $p = $p.Trim('"',"'",' ')
        if (-not (Test-Path -LiteralPath $p)) {
            Write-Host "    파일 없음: $p" -ForegroundColor Yellow
            continue
        }
        $refs.Add((Resolve-Path -LiteralPath $p).Path)
    }
    return ,$refs.ToArray()
}

function Run-Single {
    Write-Title "단일 / 다회 이미지 생성"
    $topic = Read-Default "토픽 (출력 폴더 이름)" "test-image"
    $count = Read-IntDefault "개수" 1 1 100
    $size  = Read-Size
    $quality = Read-Quality
    $prompt = Read-MultilinePrompt
    if ([string]::IsNullOrWhiteSpace($prompt)) {
        Write-Host "프롬프트가 비어 있어 취소합니다." -ForegroundColor Yellow
        return
    }
    $refs = Read-References

    Write-Host ""
    Write-Host "요약:" -ForegroundColor Cyan
    Write-Host "  topic    = $topic"
    Write-Host "  count    = $count"
    Write-Host "  size     = $size"
    Write-Host "  quality  = $quality"
    Write-Host "  refs     = $($refs.Count) 개"
    $preview = if ($prompt.Length -gt 80) { $prompt.Substring(0,80) + '...' } else { $prompt }
    Write-Host "  prompt   = $preview"
    if (-not (Read-YesNo "실행하시겠습니까? (y/n)" "y")) { return }

    $okCount = 0; $failCount = 0
    for ($i = 1; $i -le $count; $i++) {
        $name = if ($count -eq 1) { "$topic.png" } else { "${topic}_$($i.ToString('D3')).png" }
        $argList = @($GenImage, $prompt, '--topic', $topic, '--topic-root', $ProjectRoot, '-o', $name, '--size', $size, '--quality', $quality)
        foreach ($r in $refs) { $argList += @('--reference-image', $r) }
        Write-Host ""
        Write-Host "[$i/$count] 생성 중: $name" -ForegroundColor DarkGray
        & python @argList
        if ($LASTEXITCODE -eq 0) {
            $okCount++
        } else {
            $failCount++
            Write-Host "  실패." -ForegroundColor Yellow
            if ($i -lt $count) {
                if (-not (Read-YesNo "계속 진행할까요? (y/n)" "y")) { break }
            }
        }
    }
    $topicSlug = Convert-SafeSlug $topic
    $outDir = Join-Path $ProjectRoot "output\$topicSlug"
    Write-Host ""
    Write-Host "완료: 성공 $okCount, 실패 $failCount" -ForegroundColor Green
    Write-Host "출력: $outDir"
    if (Read-YesNo "탐색기에서 열까요? (y/n)" "n") { explorer $outDir }
}

function Run-Batch {
    Write-Title "배치 (preset) 생성"
    $files = @(Get-ChildItem -Path $PresetDir -Filter '*.json' | Sort-Object Name)
    if ($files.Count -eq 0) {
        Write-Host "preset 파일이 없습니다." -ForegroundColor Yellow
        return
    }
    Write-Host "Preset 선택:"
    for ($i = 0; $i -lt $files.Count; $i++) {
        Write-Host "  [$($i + 1)] $($files[$i].Name)"
    }
    $idx = Read-IntDefault "번호" 1 1 $files.Count
    $preset = $files[$idx - 1].FullName

    $count = Read-IntDefault "개수" 10 1 10000
    $conc  = Read-IntDefault "동시 워커 수" 8 1 32
    $resume = Read-YesNo "이어하기 (resume)? (y/n)" "n"
    $dry    = Read-YesNo "dry-run (계획만)? (y/n)" "n"

    $argList = @($GenBatch, $preset, '--count', $count, '--concurrency', $conc, '--topic-root', $ProjectRoot)
    if ($resume) { $argList += '--resume' }
    if ($dry)    { $argList += '--dry-run' }

    Write-Host ""
    Write-Host "실행: python $($argList -join ' ')" -ForegroundColor DarkGray
    & python @argList
}

function Show-MainMenu {
    $masked = if ($env:CLIPROXY_API_KEY) {
        $env:CLIPROXY_API_KEY.Substring(0,[Math]::Min(4,$env:CLIPROXY_API_KEY.Length)) + ('*' * 8)
    } else { '(없음)' }
    Write-Host ""
    Write-Host "===== AI 이미지 생성기 =====" -ForegroundColor Cyan
    Write-Host "  base url = $env:CLIPROXY_BASE_URL"
    Write-Host "  api key  = $masked"
    Write-Host ""
    Write-Host "  [1] 단일/다회 이미지 (직접 프롬프트)"
    Write-Host "  [2] 배치 (preset JSON)"
    Write-Host "  [q] 종료"
}

# === main ===
Ensure-Env
while ($true) {
    Show-MainMenu
    $c = Read-Host "선택"
    if ($c -ieq 'q') { break }
    switch ($c) {
        '1' { Run-Single }
        '2' { Run-Batch }
        default { Write-Host "잘못된 선택." -ForegroundColor Yellow }
    }
}
