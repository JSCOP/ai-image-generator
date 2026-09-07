# ai-image-generator SSOT

이 저장소가 이미지 생성 도구/스킬/운영 규칙의 유일한 정본입니다. 어떤 머신에서도 파일을 따로 복사하지 않고, `git pull` 하나로 모든 에이전트가 같은 내용을 봅니다.

- 정본 원격: `https://github.com/JSCOP/ai-image-generator` (`main`)
- 정본 파일: `skills/ai-image-generator/SKILL.md`, `tools/ai_image.py`, `tools/image_studio.py`, `tools/image_studio_web/*`, `open-image-studio.*`, `scripts/*`, `presets/*`, 이 문서
- 금지: 에이전트 스킬 폴더에 `SKILL.md`를 복사해 두는 것. 링크만 사용합니다.

## 머신

| 호스트 | Tailscale | 저장소 경로 | Python | CLIProxyAPI |
|---|---|---|---|---|
| `DESKTOP-SB818KQ` | `100.98.54.122` | `E:/ai-image-generator` | `python` | 이 머신에서 구동 (`0.0.0.0:8317`) |
| `JS` | `100.67.54.25` | `C:/Users/jisung/workspaces/ai-image-generator` | `py` (PATH의 `python`은 깨진 hermes venv) | 없음. 위 머신을 호출 |

`CLIPROXY_BASE_URL`은 CLIProxyAPI 호스트에서 `http://127.0.0.1:8317/v1`, 그 외 머신에서 `http://100.98.54.122:8317/v1`. 에이전트/CLI는 `CLIPROXY_API_KEY`를 프로세스 또는 Windows 사용자 환경변수에서 읽고 저장소에 절대 기록하지 않습니다.

사람이 직접 쓰는 웹 화면은 `open-image-studio.cmd`로 실행합니다. Python 3.11+와 Pillow 외에 에이전트가 필요하지 않습니다. 화면의 연결 설정은 프록시 주소만 Git 제외 파일 `config/image-studio.local.json`에 저장하며, 입력한 API 키는 서버 세션 메모리에만 보관합니다. 다른 PC에서도 clone 후 같은 런처를 사용하되, 그 PC에서 접근 가능한 프록시 주소와 인증키를 설정해야 합니다. 시작프로그램 등록·해제와 사용법은 `README.md`의 로컬 이미지 스튜디오 절을 따릅니다.

## 스킬 배포

에이전트 스킬 폴더는 저장소 `skills/<name>`을 가리키는 디렉터리 링크(Windows junction)입니다.

```powershell
git pull
python scripts/sync_skills.py          # 링크 생성/복구. 실제 폴더는 .bak-<타임스탬프>로 백업
python scripts/sync_skills.py --check  # 상태만 확인. 어긋나면 exit 1
```

대상 루트는 존재하는 것만 처리합니다: `~/.omp/agent/skills`, `~/.claude/skills`, `~/.codex/skills`, `~/.agents/skills`.

## 사용 가능한 이미지 모델

CLIProxyAPI `/v1/models` 기준 이미지 모델은 6개이며, 실제 생성까지 확인된 것은 5개입니다. (검증일 2026-09-04)

| 모델 | 경로 | 참조 이미지 | 상태 |
|---|---|---|---|
| `grok-imagine-image-2.0` | xAI `/v1/images/generations` | 불가 | 정상 (기본값) |
| `grok-imagine-image-quality` | xAI `/v1/images/generations` | 불가 | 정상 |
| `grok-imagine-image` | xAI `/v1/images/generations` | 불가 | 정상 |
| `gemini-3.1-flash-image` | Gemini native `generateContent` | 가능 | 정상. 2026-08-28 검증 |
| `gpt-image-2` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 정상. 공식 최신 모델, generation/edit 2026-09-04 검증 (`gpt-image-2-2026-04-21`) |
| `gpt-image-1.5` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 직접 API 경로 지원, 최근 실생성 미검증 |

Grok 모델은 `reference_images`가 있으면 API 호출 전에 거부합니다. `gpt-image-2`는 Responses 이미지 도구를 거치지 않고 전용 Image API로 호출하며, 참조 편집에는 `reference_images`와 `action: "edit"`를 사용합니다.

Gemini native 해상도는 요청 `size`의 긴 변으로 `512`/`1K`/`2K`/`4K` 버킷을 고르고, 최종 파일은 요청한 정확한 픽셀로 저장됩니다.

## 동기화 절차

1. 변경은 항상 저장소에서 하고 `main`에 push 합니다.
2. 각 머신에서 `git pull` 후 `python scripts/sync_skills.py --check`.
3. 원격 머신에 GitHub 자격증명이 없으면 `git bundle`로 전달합니다.

```powershell
# 보내는 쪽
git bundle create sync.bundle main --not <상대가 이미 가진 커밋>
scp -P 9991 sync.bundle jisung@100.67.54.25:C:/Users/jisung/AppData/Local/Temp/sync.bundle
# 받는 쪽
git fetch --force "$env:TEMP\sync.bundle" "main:refs/remotes/origin/main"
git merge --ff-only refs/remotes/origin/main
```

## 검증

```powershell
python -m unittest discover -s tests -v   # 원격에서는 py, 웹 런처 환경에서는 .venv\Scripts\python.exe
python scripts/sync_skills.py --check
```

실제 생성 확인은 `tools/ai_image.py --spec <파일>`이 `"ok": true`와 실제 경로를 반환하는지, 저장된 파일이 요청한 크기와 포맷인지까지 확인합니다.
