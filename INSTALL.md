# 다른 PC 설치 · AI 설치 지시 · 업데이트

정본 저장소: <https://github.com/JSCOP/ai-image-generator> (`main`). Windows를 기본 경로로 안내하며 macOS/Linux 수동 명령도 아래에 있습니다. 설치 위치는 자유입니다. 기존 PC의 `E:` 드라이브나 사용자 이름을 만들 필요가 없습니다.

## 1. 준비 사항

- Git, 실행 가능한 Python **3.11 이상**, 패키지 다운로드를 위한 인터넷.
- 이미지를 실제로 생성하려면 접근 가능한 **CLIProxyAPI 서버**, 그 서버의 API 키, 이미지 제공자 인증/크레딧이 필요합니다. 이 저장소는 이미지 클라이언트와 Studio/MCP/스킬을 설치합니다. CLIProxyAPI 자체와 계정/OAuth 인증은 포함하지 않습니다.
- 스킬을 사용할 경우 Codex, Claude Code 또는 OMP 중 사용할 에이전트가 필요합니다. 웹 Studio만 사용할 때는 에이전트가 필요 없습니다.

먼저 PowerShell에서 확인합니다. `python`이 다른 앱의 깨진 가상환경을 가리킬 수 있으므로 Windows에서는 `py -3`를 우선 사용합니다.

```powershell
git --version
py -3 --version
```

명령이 없으면 Git/Python을 설치하고 터미널을 다시 엽니다. `py`가 없는 환경에서는 정상 동작하는 `python` 또는 Python 실행 파일의 절대 경로로 대체합니다. 여러 버전이 있으면 `py -3.13`처럼 3.11 이상을 지정할 수 있습니다.

## 2. 내려받기와 설치

아래 명령은 현재 디렉터리에 프로젝트를 만듭니다. ZIP 다운로드보다 `git clone`을 권장합니다. ZIP에는 `.git`이 없어 이후 `git pull`이 동작하지 않습니다. 공개 저장소이므로 다운로드에는 GitHub 로그인이나 `jscop` 계정이 필요 없습니다.

```powershell
git clone https://github.com/JSCOP/ai-image-generator.git
cd ai-image-generator
py -3 scripts/setup.py --agent codex
py -3 scripts/setup.py --agent codex --check
```

`setup.py`는 현재 clone의 `.venv` 생성, 의존성 설치, 선택한 에이전트의 스킬 폴더 연결, CLI 스키마와 생성 계획의 dry-run 검증을 수행합니다. 이미지 생성 API를 호출하지 않습니다. `.venv`가 이미 있으면 재사용하며, 손상된 환경은 삭제하지 않고 오류를 보고합니다. 가상환경 활성화와 관리자 권한은 필요하지 않습니다.

사용 목적에 맞게 선택합니다.

| 목적 | 프로젝트 폴더에서 실행 |
|---|---|
| 웹 Studio / CLI만 | `py -3 scripts/setup.py` |
| Codex 스킬 | `py -3 scripts/setup.py --agent codex` |
| Claude Code 스킬 | `py -3 scripts/setup.py --agent claude` |
| OMP 스킬 | `py -3 scripts/setup.py --agent omp` |
| Codex + Claude | `py -3 scripts/setup.py --agent codex --agent claude` |
| Codex 스킬 + MCP 의존성 | `py -3 scripts/setup.py --agent codex --mcp` |

검증할 때는 설치 때와 동일한 옵션 뒤에 `--check`를 붙입니다. 성공하면 `Verified checkout`을 출력합니다. 이것은 **로컬 설치 검증**이며 프록시 연결/실생성 검증과는 구분합니다.

## 3. 프록시 주소와 키

새 PC에서 사용할 서버 주소를 명시하세요. 예를 들어 그 PC에서 직접 CLIProxyAPI를 실행 중이면:

```powershell
$env:CLIPROXY_BASE_URL = 'http://127.0.0.1:8317/v1'
```

기존 JSCOP 프록시 PC를 공유할 때는 **같은 Tailscale 사설망에 접속되어 있고 서버가 실행 중인 경우에만** 다음 주소를 사용합니다.

```powershell
$env:CLIPROXY_BASE_URL = 'http://100.98.54.122:8317/v1'
Test-NetConnection 100.98.54.122 -Port 8317
```

이는 기존 운영 환경의 주소이며 모든 PC에서 접근 가능한 공개 서비스가 아닙니다. 다른 서버를 쓰면 실제 HTTPS/사설망 URL로 바꿉니다. 상세한 기존 머신 정보는 [docs/SSOT.md](docs/SSOT.md)를 참조하세요. 코드의 역사적 기본값은 알려진 프록시 호스트에서 loopback, 나머지 호스트에서 위 Tailscale 주소이므로 새 설치에서는 `CLIPROXY_BASE_URL`을 직접 설정하는 편이 명확합니다.

키는 AI 채팅이나 명령문에 적지 않고 로컬 PowerShell의 숨김 입력으로 넣을 수 있습니다.

```powershell
$proxySecret = Read-Host 'CLIProxyAPI API key' -AsSecureString
$env:CLIPROXY_API_KEY = [System.Net.NetworkCredential]::new('', $proxySecret).Password
Remove-Variable proxySecret
```

설정은 현재 터미널과 그 터미널에서 시작하는 프로세스에 적용됩니다. 매번 입력하지 않으려면 본인이 다음 명령으로 Windows 사용자 환경변수에 저장할 수 있습니다. 사용자 환경변수는 암호화된 비밀 저장소가 아닙니다.

```powershell
[Environment]::SetEnvironmentVariable('CLIPROXY_BASE_URL', $env:CLIPROXY_BASE_URL, 'User')
[Environment]::SetEnvironmentVariable('CLIPROXY_API_KEY', $env:CLIPROXY_API_KEY, 'User')
```

이미 열려 있는 에이전트/MCP 앱은 다시 실행해 새 환경을 받습니다. 이 프로젝트의 CLI는 Windows 사용자 환경변수도 읽습니다. `.env` 파일을 만드는 방식은 필요 없으며, 이 도구가 `.env`를 자동으로 불러오지도 않습니다. Studio의 연결 설정에서 키를 직접 넣으면 해당 Studio 서버 메모리에서만 유지됩니다. CLI/MCP에는 별도로 환경변수가 필요합니다.

연결과 사용 가능한 이미지 모델을 확인합니다. 이미지 생성이나 파일 저장은 하지 않습니다.

```powershell
.\.venv\Scripts\python.exe tools/ai_image.py --list-models
```

`ok: true`와 실제 `available_models`가 나와야 합니다. 모델 목록 조회 성공만으로 생성 크레딧까지 보장되지는 않습니다. 실제 생성 테스트를 원하는 경우 모델 ID를 명시해 한 장을 요청하고 결과 JSON, 이미지 존재, 크기를 확인합니다.

## 4. 웹 화면 실행

```powershell
.\open-image-studio.cmd
```

브라우저에서 `http://127.0.0.1:8766/`을 엽니다. 연결 설정을 확인한 뒤 모델과 프롬프트를 선택합니다. 바로가기/자동 시작은 사용자가 원할 때만 [README.md](README.md)의 명령으로 등록합니다. 설치 스크립트는 자동 시작이나 바탕화면 등록을 하지 않습니다.

## 5. 스킬 링크와 갱신 방식

스킬 파일을 에이전트마다 복사하지 않고 **저장소 폴더를 가리키는 링크**를 만듭니다. Windows에서는 junction을 사용하므로 관리자 권한/개발자 모드가 필요 없습니다. macOS/Linux에서는 심볼릭 링크를 만듭니다.

| 옵션 | 연결 위치 |
|---|---|
| `--agent codex` | `~/.agents/skills/ai-image-generator` |
| `--agent claude` | `~/.claude/skills/ai-image-generator` |
| `--agent omp` | `~/.omp/agent/skills/ai-image-generator` |

Codex 사용자 스킬과 링크 지원은 [공식 문서](https://learn.chatgpt.com/docs/build-skills#where-codex-loads-local-skills)를 참고했습니다. 기존 `~/.codex/skills`는 기본 동기화의 호환 대상으로 유지하며, 신규 Codex 설치는 `~/.agents/skills`를 선택합니다.

스킬만 설치/복구할 수도 있습니다. `--agent`/`--root`를 명시하면 빈 PC에서도 대상 루트를 생성합니다. 기본 인자 없는 동기화는 기존 에이전트 루트만 처리합니다.

```powershell
.\.venv\Scripts\python.exe scripts/sync_skills.py --agent codex --agent claude
.\.venv\Scripts\python.exe scripts/sync_skills.py --agent codex --agent claude --check --json
```

커스텀 에이전트 경로는 `--root 'D:\my-agent\skills'`를 사용합니다. 기존 같은 이름의 실제 폴더/파일은 `~/.ai-image-generator/skill-backups/`로 보존하고 출력에 백업 경로를 표시합니다. 서로 다른 위치의 같은 이름 스킬이 선택창에 중복 표시되면 실제 스킬 경로를 확인하세요. 새 설치는 필요한 대상만 선택하면 됩니다.

스킬의 연결 원본은 `<clone>/skills/ai-image-generator/`입니다. `git pull`로 원본이 바뀌면 모든 링크에서 바로 최신 파일을 읽습니다. 에이전트에 반영되지 않으면 새 세션을 시작하거나 앱을 재시작합니다. clone을 삭제/이동하면 링크가 깨지므로 새 위치에서 다시 setup을 실행합니다.

## 6. 업데이트

프로젝트 폴더에서 현재 생성 작업이 끝난 뒤 Studio/MCP를 종료합니다. 다음은 Codex+MCP를 설치한 예입니다. 실제로 설치한 옵션과 일치시킵니다.

```powershell
git status --short
git pull --ff-only origin main
py -3 scripts/setup.py --agent codex --mcp
py -3 scripts/setup.py --agent codex --mcp --check
.\.venv\Scripts\python.exe -m unittest discover -s tests -v
```

추적 파일의 로컬 수정 또는 pull 충돌이 있으면 변경을 보존한 상태에서 원인을 보고합니다. `reset --hard`/`clean`으로 해결하지 않습니다. `.venv`, 키/로컬 설정, 결과 이미지, 참조 원본은 Git 배포 대상이 아니며 업데이트에서도 보존합니다. 기존 `.venv`가 손상되었거나 다른 PC에서 복사한 환경이면 이름을 바꿔 보관한 뒤 해당 PC에서 setup을 다시 실행하세요. 같은 clone 경로라도 가상환경은 PC마다 새로 만듭니다.

## 7. MCP 연결 (선택)

스킬/Studio 사용에 MCP는 필수가 아닙니다. MCP를 사용할 때 `setup.py --mcp`로 설치하고 [mcp.json.example](mcp.json.example)의 경로를 **현재 PC의 절대 경로**로 치환합니다.

```json
{
  "mcpServers": {
    "ai-image-generator": {
      "command": "C:/work/ai-image-generator/.venv/Scripts/python.exe",
      "args": ["C:/work/ai-image-generator/tools/mcp_server.py"],
      "env": {}
    }
  }
}
```

클라이언트의 설정 형식/위치는 앱마다 다르므로 기존 MCP 설정에 이 서버 항목만 병합합니다. Codex처럼 TOML을 쓰는 클라이언트에는 다음 서버 항목에 해당합니다. 기존 파일 전체를 덮어쓰지 않습니다.

```toml
[mcp_servers.ai-image-generator]
command = "C:/work/ai-image-generator/.venv/Scripts/python.exe"
args = ["C:/work/ai-image-generator/tools/mcp_server.py"]
env_vars = ["CLIPROXY_BASE_URL", "CLIPROXY_API_KEY"]
```

Codex의 설정 형식과 `env_vars` 전달 방식은 [공식 MCP 문서](https://learn.chatgpt.com/docs/extend/mcp?surface=cli#configure-with-configtoml)를 참고했습니다. 클라이언트가 서버 프로세스에 `CLIPROXY_BASE_URL`과 `CLIPROXY_API_KEY`를 전달하도록 환경 전달 설정을 확인하거나 Windows 사용자 환경변수를 설정합니다. 키를 MCP JSON/TOML에 적지 않습니다. 다른 프로젝트의 출력/참조 파일을 쓰려면 `args`에 `"--allow-root", "D:/work/my-project"`를 추가합니다. 서버마다 다른 state 디렉터리가 필요하면 `"--state-dir", "D:/work/my-project/ImageGallery/metadata/_mcp-codex"`를 지정합니다. 같은 state를 여러 MCP 서버가 공유하지 않습니다.

검증은 클라이언트에서 initialize와 도구 목록 조회, `plan_generation` 호출까지 확인합니다. 계획만으로 이미지 생성 API를 호출하지 않습니다. 설치 점검만 할 때 stdio 서버를 터미널에서 무작정 실행하면 입력을 기다리므로 `tools/mcp_server.py --help`를 사용하세요.

## 8. macOS / Linux

Python 3.11+의 `venv`/pip가 있는 환경에서:

```sh
git clone https://github.com/JSCOP/ai-image-generator.git
cd ai-image-generator
python3 scripts/setup.py --agent codex
python3 scripts/setup.py --agent codex --check
export CLIPROXY_BASE_URL='https://your-proxy.example/v1'
# 키는 로컬 비밀 설정으로 CLIPROXY_API_KEY에 넣습니다.
.venv/bin/python tools/ai_image.py --list-models
.venv/bin/python tools/image_studio.py --open
```

이 플랫폼은 Windows 사용자 환경변수를 읽지 않으므로 프로세스 환경변수를 사용합니다. MCP `command`는 `<clone>/.venv/bin/python`입니다. `venv` 생성 오류가 나면 OS의 Python venv/pip 구성부터 확인합니다. Windows `.cmd`/바로가기 기능은 해당되지 않습니다.

## 9. AI에게 복사해서 줄 설치 요청문

아래 요청은 **로컬 파일/터미널을 실행할 수 있는 AI 에이전트**에 전달합니다. 일반 채팅만 가능한 AI에서는 명령 안내까지만 할 수 있습니다. 설치 폴더와 에이전트 종류는 실제 환경으로 바꿉니다.

```text
https://github.com/JSCOP/ai-image-generator 를 이 PC의 C:\work\ai-image-generator 에 설치해줘.
이미 clone이 있으면 그 폴더를 재사용하고 로컬 변경을 보존해줘.
clone 후 AGENTS.md와 INSTALL.md를 읽고, 실행 가능한 Python 3.11+를 확인한 뒤
scripts/setup.py --agent codex 로 .venv와 의존성, Codex 스킬 링크를 설치해줘.
같은 옵션에 --check를 붙이고 회귀 테스트로 로컬 설치를 검증해줘.
프록시는 기존 환경변수를 우선 확인하고, 주소나 키가 없으면 필요한 정보만 알려줘.
키는 채팅/파일/로그에 기록하지 말고 내가 로컬에서 입력하게 안내해줘.
프록시가 설정되면 .venv의 Python으로 tools/ai_image.py --list-models를 확인해줘.
실제 이미지 생성 테스트는 내가 요청할 때 진행하고,
설치 경로, 스킬 링크 상태, 연결 확인 결과, 남은 설정을 구분해서 보고해줘.
```

Claude Code를 쓰면 `--agent codex`를 `--agent claude`로 바꿉니다. 둘 다 쓰면 `--agent codex --agent claude`를 지정합니다. MCP까지 원하면 다음 문장을 추가합니다.

```text
--mcp 옵션으로 MCP 의존성도 설치하고, 내 AI 클라이언트의 기존 설정을 보존하면서
현재 clone과 .venv의 절대 경로로 ai-image-generator stdio 서버를 등록해줘.
클라이언트 연결 후 도구 목록과 plan_generation까지 확인해줘.
```

기존 JSCOP 프록시를 쓸 PC라면 다음 문장을 추가합니다.

```text
프록시 주소는 http://100.98.54.122:8317/v1 이고 같은 Tailscale 사설망을 사용해.
먼저 8317 포트 접근을 확인하고, API 키는 내가 이 PC의 로컬 입력창에서 넣을게.
```

이후 업데이트 요청:

```text
이 PC의 ai-image-generator를 업데이트해줘. INSTALL.md의 업데이트 절차에 따라
로컬 변경을 보존하고 git pull --ff-only origin main을 실행해줘.
기존에 설치한 에이전트/MCP 옵션으로 setup과 --check를 실행해 의존성과 스킬 링크를 갱신하고,
회귀 테스트와 모델 목록 조회를 확인해줘. 실제 이미지 생성은 요청할 때 진행해줘.
```

## 10. 오류별 확인

| 증상 | 확인할 것 |
|---|---|
| `py` 없음 / `python` 실행 실패 | Python 3.11+ 설치, 터미널 재시작, 실행 파일 절대 경로. 기존 앱의 venv를 재사용하지 않습니다. |
| setup `--check` 실패 | 같은 옵션으로 setup을 먼저 실행. `.venv` Python과 Pillow/MCP 설치 오류 확인. |
| 스킬이 안 보임 | 명시적 `--agent`로 링크 설치, `sync_skills.py --agent ... --check --json`, 새 에이전트 세션. |
| 연결 시간 초과 / connection refused | 실제 프록시 실행 여부, URL/포트, Tailscale 로그인, 네트워크/방화벽. |
| HTTP 401/403 | 해당 CLIProxyAPI 서버에 설정된 키와 제공자 인증. 웹 계정 비밀번호를 API 키로 쓰지 않습니다. |
| 모델이 없음 / 크레딧 오류 | `--list-models`의 실제 목록과 해당 제공자 계정/크레딧. 다른 모델로 몰래 바꾸지 않습니다. |
| Studio 포트 충돌 | `.\open-image-studio.cmd -Port 8776`. |
| MCP 파일 접근 거부 | 해당 프로젝트/참조 폴더를 `--allow-root`로 지정. |
| MCP state 잠금 오류 | 같은 state의 기존 서버를 종료하거나 클라이언트별 `--state-dir` 지정. |
| Git pull 실패 | `git status`와 원격/브랜치를 확인하고 로컬 변경을 보존. ZIP이면 새 clone으로 설치. |

## 검증 범위

2026-10-07: Windows의 별도 clone(공백 포함 경로), 새 사용자 홈, 새 `.venv`에서 Python 3.13.5 / Pillow 12.3.0 / MCP SDK 1.30.0으로 Codex+Claude+MCP 설치와 `--check`, 링크 상태, 회귀 테스트 **33개**가 통과했습니다. 실제 MCP stdio 클라이언트의 initialize, 도구 **20개** 조회, `plan_generation`도 통과했습니다. 이 검증은 외부 이미지 생성 API를 호출하지 않았습니다. macOS/Linux 명령은 제공하지만 이번에는 해당 OS에서 실행 검증하지 않았습니다. 각 PC의 프록시/키/생성 크레딧은 별도로 확인해야 합니다.
