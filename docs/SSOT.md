# ai-image-generator SSOT

이 저장소가 이미지 생성 도구/스킬/운영 규칙의 유일한 정본입니다. 어떤 머신에서도 파일을 따로 복사하지 않고, `git pull` 하나로 모든 에이전트가 같은 내용을 봅니다.

- 정본 원격: `https://github.com/JSCOP/ai-image-generator` (`main`)
- 정본 파일: `skills/ai-image-generator/SKILL.md`, `tools/ai_image.py`, `tools/image_core.py`, `tools/mcp_*.py`, `scripts/*`, `presets/*`, 이 문서
- 금지: 에이전트 스킬 폴더에 `SKILL.md`를 복사해 두는 것. 링크만 사용합니다.

신규 PC의 설치·연결·업데이트와 AI에게 줄 요청문은 [../INSTALL.md](../INSTALL.md)가 설치 절차의 정본입니다. 스킬은 로드된 `SKILL.md` 링크의 실제 경로에서 clone을 찾으며 머신 이름이나 드라이브에 의존하지 않습니다.

## 기존 운영 머신 (신규 PC의 필수 경로 아님)

| 호스트 | Tailscale | 저장소 경로 | Python | CLIProxyAPI |
|---|---|---|---|---|
| `DESKTOP-SB818KQ` | `100.98.54.122` | `E:/ai-image-generator` | `python` | 이 머신에서 구동 (`0.0.0.0:8317`) |
| `JS` | `100.67.54.25` | `C:/Users/jisung/workspaces/ai-image-generator` | `py` (PATH의 `python`은 깨진 hermes venv) | 없음. 위 머신을 호출 |

`CLIPROXY_BASE_URL`은 CLIProxyAPI 호스트에서 `http://127.0.0.1:8317/v1`, 그 외 머신에서 `http://100.98.54.122:8317/v1`. 에이전트/CLI는 `CLIPROXY_API_KEY`를 프로세스 또는 Windows 사용자 환경변수에서 읽고 저장소에 절대 기록하지 않습니다.

기존 웹 Studio, PowerShell 메뉴, 수동 크롭 도구, 갤러리 이전 도구는 2026-10-18에 제거했습니다(마지막 포함 커밋 `52e220b`). 사람용 Studio는 새로 제작합니다. CLI와 MCP가 함께 쓰는 모델 카탈로그·연결 확인·프롬프트 합성은 `tools/image_core.py`에 있습니다.

Rust 이전(설계 `docs/superpowers/specs/2026-10-18-image-studio-rust-app-design.md`): 1단계 `crates/image-core`·`crates/image-cli`가 같은 JSON 계약으로 구현되어 있고 Python과 동등성 테스트를 통과합니다. 에이전트·MCP는 아직 Python을 씁니다. Rust CLI는 `CLIPROXY_BASE_URL`이 반드시 필요하며, 2026-10-18에 `DESKTOP-SB818KQ`의 Windows 사용자 환경변수를 `http://127.0.0.1:8317/v1`로 설정했습니다. Rust CLI 실검증(2026-10-18, CLIProxyAPI 경유): `gpt-image-2.5` 생성, `gpt-image-2.5-sunburst` 투명 생성(실제 RGBA, contain) 성공. 같은 날 이 PC의 Tailscale은 `NoState`(조정 서버 연결 불가)여서 노트북 경로는 미검증입니다.

## 스킬 배포

에이전트 스킬 폴더는 저장소 `skills/<name>`을 가리키는 디렉터리 링크(Windows junction)입니다.

```powershell
git pull
python scripts/sync_skills.py          # 기존 루트의 링크 생성/복구. 실제 폴더는 ~/.ai-image-generator/skill-backups에 백업
python scripts/sync_skills.py --check  # 상태만 확인. 어긋나면 exit 1
```

대상 루트는 존재하는 것만 처리합니다: `~/.omp/agent/skills`, `~/.claude/skills`, `~/.codex/skills`, `~/.agents/skills`.

신규 PC에서는 `python scripts/setup.py --agent codex`처럼 대상을 명시하여 없는 루트도 생성합니다. 신규 Codex 대상은 `~/.agents/skills`이며 Claude/OMP와 커스텀 루트는 `INSTALL.md`를 따릅니다. 기본 동기화가 모든 루트를 건너뛰면 exit 1이며, 명시적 대상의 `--check`는 루트가 없을 때도 exit 1입니다. 파일을 복사하는 대신 링크로 연결하여 이후 pull을 바로 반영합니다.

## 사용 가능한 이미지 모델

CLIProxyAPI `/v1/models` 기준 이미지 모델은 9개이며, 실제 생성까지 확인된 것은 8개입니다. (검증일 2026-09-30)

| 모델 | 경로 | 참조 이미지 | 상태 |
|---|---|---|---|
| `grok-imagine-image-2.0` | xAI `/v1/images/generations` | 불가 | 정상 |
| `grok-imagine-image-quality` | xAI `/v1/images/generations` | 불가 | 정상 |
| `grok-imagine-image` | xAI `/v1/images/generations` | 불가 | 정상 |
| `gemini-3.1-flash-image` | Gemini native `generateContent` | 가능 | 정상. 2026-08-28 검증 |
| `gpt-image-2.5` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 정상 (기본값). 최신 GPT 이미지 모델, generation/edit 2026-09-30 검증 |
| `gpt-image-2.5-flare` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 정상. generation 2026-09-30 검증 |
| `gpt-image-2.5-sunburst` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 정상. generation 2026-09-30 검증 |
| `gpt-image-2` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 정상. 이전 세대, generation/edit 2026-09-04 검증 (`gpt-image-2-2026-04-21`) |
| `gpt-image-1.5` | OpenAI `/v1/images/generations`, `/v1/images/edits` | 가능 | 직접 API 경로 지원, 최근 실생성 미검증 |

Grok 모델은 `reference_images`가 있으면 API 호출 전에 거부합니다. `gpt-image-*`(2.5 계열 포함)는 Responses 이미지 도구를 거치지 않고 전용 Image API로 호출하며, 참조 편집에는 `reference_images`와 `action: "edit"`를 사용합니다.

Gemini native 해상도는 요청 `size`의 긴 변으로 `512`/`1K`/`2K`/`4K` 버킷을 고르고, 최종 파일은 요청한 정확한 픽셀로 저장됩니다.

## 병렬 생성 실측 (2026-10-18, `DESKTOP-SB818KQ`)

CLIProxyAPI `routing.strategy: fill-first`에서는 동시 요청이 우선순위가 가장 높은 Codex 계정 하나로 모두 갑니다. 그 상태에서 `gpt-image-2.5`를 `/v1/images/*`로 동시에 보냈습니다(관리 API 계정별 success 카운터로 단일 계정 처리 확인).

| 동시 요청 | 종류 | 성공 | 전체 시간 | 장당 지연(최소/중앙/최대) | 처리량 |
|---|---|---|---|---|---|
| 1 | generate, 1024x1024 medium | 1/1 | 20.0초 | 20.0초 | 3.0장/분 |
| 4 | generate, 1024x1024 medium | 4/4 | 27.2초 | 18.5 / 22.7 / 27.2초 | 8.8장/분 |
| 8 | generate, 1024x1024 medium | 8/8 | 25.0초 | 18.6 / 21.9 / 25.0초 | 19.2장/분 |
| 12 | generate, 1024x1024 medium | 12/12 | 27.5초 | 18.6 / 21.5 / 27.5초 | 26.2장/분 |
| 1 | generate, 1920x1088 high | 1/1 | 17.3초 | 17.3초 | 3.5장/분 |
| 10 | generate, 1920x1088 high | 10/10 | 28.2초 | 18.5 / 21.1 / 28.2초 | 21.3장/분 |
| 30 | generate, 1920x1088 high | 30/30 | 33.8초 | 17.5 / 22.7 / 33.8초 | 53.3장/분 |
| 10 | edit(참조 1장), 1920x1088 high | 10/10 | 34.6초 | 24.5 / 29.1 / 34.5초 | 17.3장/분 |

결론:

- 장당 지연은 동시 수와 거의 무관합니다. 병목은 계정 수가 아니라 클라이언트 동시 수였습니다. 기존 기본값(CLI 4, MCP 2, Studio·메뉴 직렬)이 30장을 수 분~10분으로 늘렸습니다.
- 기본 동시 수를 CLI/MCP/배치(당시 메뉴/Studio 포함) 모두 30(최대 32)으로 올리고, 일시적 실패(HTTP 408/425/429/5xx, 연결 끊김, 타임아웃)는 `gen_image.py` 종료 코드 3으로 구분해 백오프 재시도합니다(`retries` 기본 2).
- 프록시 설정 변경은 필요하지 않습니다. 한 계정이 사용 한도에 걸리면 `fill-first`가 다음 우선순위 계정으로 넘기고, 클라이언트 재시도가 그 전환을 흡수합니다. 사용 한도 소진 시 전환은 실제로 유도하지 않았으므로 미검증입니다.
- 관찰: 요청 크기와 무관하게 provider 원본은 항상 `1536x1024`였습니다(1024x1024 요청 포함). 최종 파일은 요청 크기로 크롭/리샘플링됩니다.

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

### Endpoint selection

`gen_image.resolve_base_url` is shared by the CLI and MCP server. It checks the actual hostname: `DESKTOP-SB818KQ` uses `http://127.0.0.1:8317/v1`; other machines default to the Tailscale endpoint. Explicit URL, process environment, then Windows user environment override the default. On the proxy host, the known Tailscale endpoint and localhost alias are normalized to `127.0.0.1`; custom endpoints are preserved.

## Gallery storage

All entry points use `<destination-project-or-workspace>/ImageGallery/output/<topic>/` for images and `metadata/<topic>/` for minimal request records. Agent requests explicitly set `topic_root` to the requested project (including newly created projects), or the active workspace when no project is requested. Without a model choice the skill queries `tools/ai_image.py --list-models` and uses `gpt-image-2.5` without a selection question. An explicitly requested model takes priority, and an explicitly selected conversation model carries into follow-ups until changed/reset to the default; no preference file is created. Exact variant IDs are passed unchanged. Ordinary generation writes one image and one JSON, optionally including the provider's `response_model`, and MCP state lives under metadata/_mcp. CLI dry-runs write no files.
