# ai-image-generator

CLIProxyAPI 기반 이미지 생성 도구입니다. 에이전트용 CLI/스킬과 선택적 MCP 서버를 제공합니다. 사람용 웹 Studio는 새로 제작할 예정입니다.

## 다른 PC에 설치하기

Git과 Python 3.11+가 있는 Windows PowerShell에서:

```powershell
git clone https://github.com/JSCOP/ai-image-generator.git
cd ai-image-generator
py -3 scripts/setup.py --agent codex
py -3 scripts/setup.py --agent codex --check
```

Claude Code는 `--agent claude`, 둘 다 쓰면 `--agent codex --agent claude`, CLI만 쓰면 `--agent`를 생략합니다. MCP 의존성은 `--mcp`를 추가합니다. 설치 경로는 자유이며 스킬은 실제 clone을 가리키는 링크로 등록됩니다.

**[INSTALL.md — 프록시 연결, 스킬 설치·업데이트, MCP, 오류 해결, AI에게 줄 설치 요청문](INSTALL.md)** 을 따라 설정하세요. 실제 생성에는 별도의 CLIProxyAPI 서버 주소·키·제공자 인증/크레딧이 필요합니다. 설치 검증은 이미지 생성 API를 호출하지 않습니다.

## MCP 서버

생성/편집/배치·이미지 조회/가공 기능은 `tools/mcp_server.py`의 표준 MCP stdio 서버로 사용할 수 있습니다. 기존 환경을 유지한 채 MCP 의존성을 추가합니다.

```powershell
py -3 scripts/setup.py --mcp
.\.venv\Scripts\python.exe tools\mcp_server.py --help
```

클라이언트 설정 예시는 [`mcp.json.example`](mcp.json.example)이며 현재 PC의 clone/.venv 절대 경로로 바꿉니다. 클라이언트가 stdio 서버를 실행합니다. 실행 시 `CLIPROXY_BASE_URL`과 `CLIPROXY_API_KEY` 환경변수를 읽습니다. 기본 파일 접근은 저장소 내부이며, 출력 프로젝트나 참조 이미지가 다른 위치에 있으면 `--allow-root "D:\refs"`를 추가합니다. 한 state 디렉터리에는 서버 하나만 실행합니다.

MCP 사용 순서는 `plan_generation` → `start_generation` → `get_job`이며, 계획 단계에서는 API를 호출하지 않습니다. `single`, 서로 다른 `jobs`, preset `batch`를 지원하고, `cancel_job`, `resume_job`, `image_preview`, `crop_image`, `resize_image`, `contact_sheet`, 프리셋 조회도 제공합니다. 결과는 출력 파일의 존재와 요청 크기를 확인한 뒤 성공으로 보고합니다. API 키는 MCP 응답·상태 파일·로그에 저장하지 않습니다. 자세한 구현 범위와 검증은 [`docs/MCP_IMPLEMENTATION.md`](docs/MCP_IMPLEMENTATION.md)에 있습니다.

기존 머신 토폴로지, 스킬 배포(링크) 규칙, 검증된 이미지 모델 상태는 `docs/SSOT.md`가 정본입니다. 신규 PC 설치는 `INSTALL.md`를 따릅니다.

## 지원 모델

- **ChatGPT 계열:** `gpt-image-2.5`(최신, 기본값), `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`, `gpt-image-2`, `gpt-image-1.5`. 참조가 있으면 이미지 편집 API를 사용합니다.
- **Gemini:** `gemini-3.1-flash-image`. 참조 이미지를 포함한 생성이 가능합니다.
- **Grok:** `grok-imagine-image-2.0`, `grok-imagine-image-quality`, `grok-imagine-image`. 현재 생성 경로에서는 참조 이미지를 지원하지 않습니다.
- 카탈로그에 있는 모델과 현재 계정에서 사용 가능한 모델은 다릅니다. `python tools/ai_image.py --list-models`가 실제 `/models` 목록을 보여 줍니다. 할당량·크레딧·제공자 상태에 따라 실생성은 실패할 수 있습니다.

## 검증

```powershell
.\.venv\Scripts\python.exe -m unittest discover -s tests -v
```

테스트는 로컬 제공자 fixture를 사용하며 외부 이미지 생성 비용을 발생시키지 않습니다.

## 기본 원칙

- 사용자가 개수를 명시하지 않으면 1장만 생성합니다. 단, 여러 공간/항목을 나열하며 `한 장씩`, `각각`, `one each`라고 한 경우는 명시 개수로 보고 항목당 1장씩 생성합니다.
- 사용자가 명시적으로 요청하지 않으면 기존 이미지를 삭제하거나 덮어쓰지 않습니다.
- 배치 생성은 먼저 `--dry-run`으로 계획을 확인한 뒤 실행합니다.
- 품질 선택 질문은 하지 않습니다. 사용자가 명시적으로 낮은 품질/초안을 요청하지 않는 한 항상 `1920x1080`, `quality: "high"`로 바로 시작합니다.
- provider가 16배수 또는 고정 해상도 버킷을 요구하면 내부 요청만 패딩/버킷 처리하고, 결과 파일은 중앙 크롭과 고품질 리샘플링으로 요청한 정확한 크기에 맞춥니다.

## 품질 기본 워크플로우

- 기본 생성은 항상 묻지 않고 `1920x1080`, `quality: "high"`, 필요한 전체 참조 이미지로 시작합니다.
- 여러 장, 참조 이미지가 많은 작업, T2IN/ImageGallery 작업에서도 품질 선택 질문 없이 최상 품질로 바로 생성합니다.
- `low`, `medium`, `draft`, `초안`, `빠른 시안`처럼 사용자가 명시적으로 낮은 품질을 요청한 경우에만 더 낮은 품질/해상도를 사용합니다.
- 여러 장은 병렬로 생성합니다. `concurrency` 기본값은 30(1~32로 제한)이라 30장까지는 별도 지정 없이 전부 동시에 생성합니다. 2026-10-18 실측에서 CLIProxyAPI Codex 계정 하나가 `gpt-image-2.5` high 동시 30장을 34초, 참조 편집 동시 10장을 35초에 처리했고 장당 지연(약 18~35초)은 동시 수와 무관했습니다. 31~32장은 `concurrency`를 장수와 같게 지정하면 됩니다.
- 429·5xx·연결 끊김·타임아웃 같은 일시적 실패는 `retries`(기본 2, 최대 5)만큼 백오프 후 자동 재시도합니다. 재시도 후에도 429가 반복될 때만 `concurrency`를 낮춥니다.
- `job_timeout_sec`는 속도 옵션이 아니라 느린 job을 언제 중단할지 정하는 제한입니다.
- 구조 보존형 참조 편집은 `gpt-image-*`와 `action: "edit"`를 사용합니다. 스타일 이미지가 구조를 오염시킬 수 있으면 구조 원본만 전달하고 스타일은 텍스트로 지정합니다.
- 정확한 도면/배치 작업은 먼저 1~2장을 생성해 원본과 비교 검수한 뒤 다량 생성합니다. 기존 일부 출력이 있으면 같은 topic에 `resume: true`를 사용해 누락분만 재개합니다.

## 설치 / 환경

PowerShell:

```powershell
$env:CLIPROXY_BASE_URL = "http://localhost:8317/v1"
$env:CLIPROXY_API_KEY = "<local-cliproxy-api-key>"
```

정확한 출력 크기 변환을 위해 Pillow를 설치합니다: `python -m pip install -r requirements.txt`.

CLIProxyAPI 확인:

```powershell
Invoke-WebRequest -UseBasicParsing `
  -Uri "$env:CLIPROXY_BASE_URL/models" `
  -Headers @{ Authorization = "Bearer $env:CLIPROXY_API_KEY" }
```

## 에이전트 권장 진입점

에이전트는 `tools/ai_image.py`를 우선 사용합니다. JSON 입력을 받고 JSON 한 줄을 반환하므로 Claude Code / Codex / Hermes에서 결과 경로를 안정적으로 파싱할 수 있습니다.

모델을 지정하지 않으면 선택 질문 없이 `gpt-image-2.5`를 사용합니다. `python tools/ai_image.py --list-models`로 연결과 모델 사용 가능 여부를 확인하고, 요청한 모델이 없으면 오류를 알립니다. 사용자가 모델을 직접 지정하면 그 정확한 ID를 우선합니다. `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`, Gemini, Grok도 명시적으로 선택할 수 있습니다. 대화에서 선택한 모델은 후속 요청에 유지하며, "기본 모델로"라고 요청하면 `gpt-image-2.5`로 돌아옵니다.

## Rust CLI (이전 중, 2단계)

같은 JSON 계약을 지키는 Rust CLI `image-cli`가 `crates/`에 있습니다. 에이전트 기본 진입점은 아직 `tools/ai_image.py`입니다(전환은 단계 6).

```powershell
cargo build --release -p image-cli
$env:CLIPROXY_BASE_URL = "http://127.0.0.1:8317/v1"   # 프로필이 없으면 필수: 머신별 기본 주소가 없습니다
target/release/image-cli --list-models
target/release/image-cli --json '{"prompt":"...","topic":"x","dry_run":true}'
```

### 연결 프로필

연결을 "프로필"(공급자 종류 + 주소 + 키)로 저장해 두고 `--profile 이름`으로 고를 수 있습니다. 프로필 목록은 `%APPDATA%\ImageStudio\profiles.json`(환경변수 `IMAGE_STUDIO_HOME`이 있으면 그 폴더)에, 키는 Windows 자격 증명 관리자에만 저장됩니다. 키는 명령줄 인자로 받지 않습니다(셸 기록에 남기 때문). `--key-env 변수`는 그 환경변수에서, `--key-stdin`은 파이프로 넘긴 표준입력 첫 줄에서 읽습니다(터미널에서 직접 입력하면 키가 화면에 보이므로 거부하고 파이프 예시를 안내합니다).

```powershell
# 이 PC에서 도는 CLIProxyAPI (/v1은 자동으로 붙음)
target/release/image-cli profile add local --kind cliproxy --url http://127.0.0.1:8317 --key-env CLIPROXY_API_KEY --default

# 노트북에서 Tailscale로 집 PC의 CLIProxyAPI 사용 (Tailscale·사설망 http는 경고 없음)
target/release/image-cli profile add home --kind cliproxy --url http://100.64.0.10:8317 --key-env CLIPROXY_API_KEY

# OpenAI 공식 API (주소는 기본값 https://api.openai.com/v1, 키 필수). 키는 숨김 입력으로 넘깁니다
$k = Read-Host 'OpenAI API key' -AsSecureString
[System.Net.NetworkCredential]::new('', $k).Password | target/release/image-cli profile add openai --kind openai --key-stdin
Remove-Variable k

target/release/image-cli profile list            # 키 값은 나오지 않고 key_configured만 표시
target/release/image-cli profile check local     # 서버 접근 → 인증 → 모델 목록 (유료 생성 없음)
target/release/image-cli --profile local --json '{"prompt":"...","topic":"x","image_model":"gpt-image-2.5"}'
```

종류는 `cliproxy`, `openai`, `gemini`, `xai`, `openai_compatible`입니다. 연결은 spec `profile` 또는 `--profile` → 프로세스 `CLIPROXY_BASE_URL` → 사용자 환경변수 `CLIPROXY_BASE_URL` → 기본 프로필 순서로 정합니다. 고른 프로필이 없으면 다른 주소·키를 빌리지 않고 호출 전에 오류를 냅니다. 키가 꼭 필요한 공식 API 종류(`openai`·`gemini`·`xai`) 프로필에 저장된 키가 없을 때도 호출 전에 오류입니다. `cliproxy`·`openai_compatible` 프로필은 키 없이도 쓸 수 있고, 이때는 인증 헤더 없이 보내며 환경변수 키를 빌리지 않습니다. 프로필 종류와 맞지 않는 모델(예: Gemini 프로필에 `gpt-image-2.5`)도 호출 전에 거부합니다. 동시 실행 기본값은 `cliproxy`·`openai_compatible` 30, 공식 API(`openai`·`gemini`·`xai`) 4이며 `profile add/update --concurrency N`이나 spec `concurrency`로 바꿉니다. 공식 API 직접 연결은 문서 근거로 구현했고 아직 실제 키로 확인하지 않았습니다. OpenAI 공식 API에는 접미사 없는 `gpt-image-2.5` ID가 없으므로 `gpt-image-2.5-sunburst`나 `-flare`를 지정하세요.

투명 배경은 `"image_model":"gpt-image-2.5-sunburst","background":"transparent"`로 요청하며 결과는 비율을 유지한 투명 여백(`fit:"contain"`)으로 저장됩니다. Python과 다른 점은 [docs/RUST_PARITY.md](docs/RUST_PARITY.md)에 있습니다. 검증: `cargo test --workspace`(Python `.venv`가 있으면 동등성 비교 포함).

## Image Studio 앱 (3단계, 1차판)

사람이 쓰는 Windows 데스크톱 앱입니다. `apps/studio/`에 있고(Tauri 2 + Svelte 5), 엔진은 Rust CLI와 같은 `image-core`를 씁니다. 연결 프로필·키 저장 위치도 CLI와 같아서, CLI로 만든 `local` 프로필이 앱에 그대로 보입니다.

```powershell
pnpm --dir apps/studio install                       # 처음 한 번
pnpm --dir apps/studio tauri dev                     # 개발 실행(창이 뜨고, 화면 코드를 고치면 바로 반영)
pnpm --dir apps/studio tauri build --debug --no-bundle   # 디버그 빌드 → target/debug/studio.exe
pnpm --dir apps/studio dev                           # 브라우저만으로 화면 확인: http://localhost:1420/?mock=1
```

`?mock=1`은 개발 서버에서만 쓰는 가짜 응답(연결 2개, 모델 6개, 갤러리 기록, 흉내 생성)입니다. `?mock=empty`는 연결이 하나도 없는 첫 실행, `?mock=env`는 환경변수 연결, `?mock=broken`은 프로필 파일을 읽지 못한 경우를 보여 줍니다. 운영 빌드에는 들어가지 않습니다.

화면 셋(왼쪽 아이콘으로 이동):

- **편집**: 결과 보기·갤러리 상세의 "편집", 참조 이미지 썸네일의 연필 버튼으로 엽니다. "영역 칠하기"로 바꿀 곳을 칠하거나 "그리기"로 대략 그린 뒤 바꿀 내용을 쓰고 "칠한 부분 수정"을 누르면, 칠한 곳 주변만 잘라 모델에 보내고(칠한 곳 위치를 알려 주는 흑백 안내 그림 포함) 결과를 칠한 곳에만 덮어씁니다. 칠한 곳 밖은 원본 픽셀 그대로입니다. CLIProxyAPI가 OpenAI `mask`를 무시하는 것을 실측(2026-10-19)해 이렇게 처리합니다. 마스크(`<topic>-mask.png`)와 그린 그림(`<topic>-drawn.png`)은 metadata 폴더에 남습니다. "내 PC에서 배경 지우기"는 ISNet(DIS `isnet-general-use`, Apache-2.0) 모델을 ONNX Runtime으로 CPU에서 돌려 원본 픽셀은 그대로 두고 배경만 투명하게 한 새 이미지를 만듭니다(처음 한 번 모델 170MB를 앱 폴더 `models/`에 내려받고 SHA-256 확인, 1024px 기준 1초 안팎). "AI로 투명 배경 다시 그리기"는 투명 배경 모델(`gpt-image-2.5`, `-sunburst`, `-flare`)로 이미지를 다시 그립니다(모양이 조금 바뀔 수 있고 비용이 듭니다).
- **생성**: 왼쪽에서 프롬프트·피할 것·연결·모델·크기·품질·장수(1~50)·참조 이미지(최대 4장, 장당 10MB)·배경(투명은 지원 모델만)·맞추는 방식을 고르고 "N장 생성". 프롬프트 아래 "영어로 다듬기"는 고른 글 모델(기본 `gpt-6-luna`, 연결의 글 모델 모두 선택 가능)로 자세한 영어 프롬프트를 만들어 미리 보여 주고, "적용"해야 바뀝니다. 적용하면 원래 쓴 요청(예: "남자 귀여운 캐릭터")은 프롬프트 아래에 남고, 생성 기록에 `original_prompt`로 함께 저장됩니다(모델에는 보내지 않음). 결과 보기·갤러리 상세에서 원래 요청과 보낸 영어 프롬프트를 따로 복사할 수 있고, "원래 요청으로 다시 다듬기"로 원래 요청을 프롬프트 칸에 되돌려 고친 뒤 다시 다듬을 수 있습니다. GPT 모델에는 정한 크기의 모양(예: "wide landscape 16:9 (1920x1080)")을 프롬프트 끝에 한 줄 붙여 보냅니다. CLIProxyAPI 뒤의 모델이 `size`를 무시하고 내용에 맞춰 세로·정사각형으로 그리는 일을 막기 위해서입니다(2026-10-19 실측: 붙이기 전 3/3 세로, 붙인 뒤 3/3 16:9). 맞추는 방식은 모델이 다른 비율로 그림을 보냈을 때 쓰며 기본은 자르지 않는 "여백 채움"(투명 여백), 그 밖에 "원본 크기"(받은 크기 그대로)와 "잘라 채움"이 있습니다. 가운데에 실행마다 장별 대기·생성 중·재시도(원인)·완료·실패(원인) 카드와 진행 막대가 나옵니다. 생성 중에도 "N장 생성"으로 다른 생성을 더 시작할 수 있고(실행마다 따로 진행·취소, 최신이 위), 끝난 실행은 ×로 목록에서 지웁니다(이미지는 남음). 실행 머리글·이미지 보기·갤러리 상세의 "프롬프트 복사"는 보낸 그대로의 프롬프트(피할 것은 `Avoid:` 줄)를 클립보드에 넣습니다. 오른쪽 기록에서 지난 결과를 다시 보거나 "이 설정 다시 쓰기"·"이어서 생성"을 고릅니다. 참조 이미지는 끌어 놓기·파일 고르기 말고 Ctrl+V로도 붙여 넣을 수 있습니다(붙여 넣은 그림은 작업 폴더 `ImageGallery/references/`에 새 파일로 저장). 연결이 없으면 "연결을 먼저 추가하세요"와 연결 화면 버튼이 나옵니다.
- **갤러리**: 작업 폴더(기본 `%USERPROFILE%\Pictures\Image Studio`)와 추가한 폴더의 결과를 날짜별로 봅니다. 폴더·상태(완료/실패/파일 없음)·검색으로 거르고, 고른 이미지의 기록(모델, 원본 크기→저장 크기, 요청 ID 등)을 오른쪽에서 봅니다. 사라진 폴더는 "찾을 수 없음"으로 표시되고 나머지는 그대로 보입니다. 폴더를 목록에서 빼도 파일은 지우지 않습니다.
- **연결**: CLIProxyAPI·OpenAI·Gemini·xAI·OpenAI 호환 연결을 추가·수정·삭제하고, "연결 확인"으로 서버·인증·모델·마지막 생성 4단계를 봅니다(이미지를 만들지 않아 비용 없음). 키는 입력만 받고 다시 보여 주지 않습니다.

앱 설정은 `%APPDATA%\ImageStudio\settings.json`(첫 실행 때 생김, `IMAGE_STUDIO_HOME`이 있으면 그 폴더)에 저장됩니다. 생성 결과는 `<작업 폴더>\ImageGallery\output\<주제>\`에 저장됩니다.

스크린샷(목업 응답, 1440×900 / 1100×700): [빈 생성 화면](apps/studio/docs/screenshots/generate-empty-1440.png) · [생성 중](apps/studio/docs/screenshots/generate-running-1440.png) · [생성 완료](apps/studio/docs/screenshots/generate-done-1440.png) · [투명 생성](apps/studio/docs/screenshots/generate-done-1100.png) · [갤러리](apps/studio/docs/screenshots/gallery-1440.png) · [연결 확인](apps/studio/docs/screenshots/connections-1440.png) · [연결 확인 실패](apps/studio/docs/screenshots/connections-1100.png) · [첫 실행](apps/studio/docs/screenshots/first-run-generate-1440.png)

검증(2026-10-19): `cargo test --workspace`, `pnpm --dir apps/studio test`·`check`·`build`, 디버그 앱 실행(창 표시, 10초 유지, 첫 실행 settings.json 생성), 앱 서비스 경로로 실제 1장 생성(`cargo test -p studio --test real_proxy -- --ignored --test-threads=1`, `local` 프로필, `gpt-image-2.5` low 1024×1024 → `ImageGallery/output/studio-app-smoke-20261018/`). 이 테스트는 실제 비용이 드는 1장을 만들고, 같은 폴더가 이미 있으면 덮어쓰지 않고 멈춥니다.

**사용자 수동 확인 목록** (앱 창에서 직접, 아직 미확인):

1. 생성: 연결 `local`, `gpt-image-2.5`, 낮음 품질, 2장 → 카드가 생성 중 → 완료로 바뀌고 이미지가 보이는지, 카드를 누르면 큰 보기에 원본 크기→저장 크기·요청 ID가 나오는지.
2. 취소: 4장 이상 생성 중 "취소" → 남은 카드가 "취소됨"으로 끝나고 다시 생성할 수 있는지.
3. 이어하기: 취소한 기록에서 "이어서 생성" → 이미 완료된 장은 건너뛰고 나머지만 만드는지(갤러리에서 같은 주제 확인).
4. 창 닫기 중 취소: 생성 중에 창을 닫기 → 몇 초 안에 창이 닫히고, 다시 열면 갤러리에서 완료/실패/끝나지 않음이 구분되며 `.lock` 파일이 남지 않는지.
5. 화면 이동: 생성 중 갤러리·연결로 갔다 돌아와도 진행이 계속 보이는지.
6. 갤러리: 다른 폴더 추가·빼기, 상태 필터, 검색, "탐색기에서 보기".

## 직접 CLI 사용

### 단일 이미지

```powershell
python scripts\gen_image.py `
  "고양이가 우주를 유영하는 시네마틱 사진" `
  --topic cat-in-space `
  -o cat.png `
  --size 1920x1080 `
  --quality high
```

저장 위치:

```text
ImageGallery/output/<topic>/<filename>.png
```

주요 옵션:

- `--topic <name>`: `ImageGallery/output/<topic>` 폴더 이름. 생략 시 prompt에서 자동 추출
- `-o <file>`: 출력 파일명 또는 경로
- `--size 1920x1080`: 최종 결과 파일의 정확한 크기
- `--quality low|medium|high`: 품질. 생략 시 `high`
- `--image-model <id>`: 이미지 provider 모델. 생략 시 `gpt-image-2.5`
- `--reference-image <path>`: 참조 이미지. 여러 번 지정 가능
- `--events <path>`: 요청한 경우에만 원본 API 응답 저장 (metadata 쪽 경로 사용)

### JSON preset 배치

```powershell
# 계획만 확인, API 호출 없음
python scripts\gen_batch.py presets\extraction-rpg.json --count 10 --dry-run

# 실제 생성
python scripts\gen_batch.py presets\extraction-rpg.json --count 10

# 이어서 생성
python scripts\gen_batch.py presets\extraction-rpg.json --count 100 --resume
```

배치 옵션:

- `--count N`: 생성 개수
- `--concurrency N`: 동시 워커 수(기본 30, 최대 32). 여러 이미지를 동시에 요청합니다. `N=30`이면 한 번에 최대 30개의 `gen_image.py` 프로세스가 선택한 provider endpoint를 호출합니다.
- `--retries N`: 일시적 실패(429·5xx·연결 끊김·타임아웃) 시 장당 추가 시도 횟수(기본 2)
- `--seed N`: 카테고리/템플릿 선택 시드
- `--topic <name>`: preset의 `topic` 덮어쓰기
- `--image-model <id>`: preset의 `image_model` 덮어쓰기. 지정하지 않으면 preset의 명시 모델을 유지하고, preset에도 없으면 `gpt-image-2.5`
- `--topic-root <dir>`: 작업 공간 또는 ImageGallery 경로
- `--resume`: 이미 존재하는 PNG는 건너뜀
- `--dry-run`: 계획 JSON만 출력하며 파일·API 요청을 만들지 않음
- `--max-failures N`: 누적 실패 시 중단

출력 구조 (단일·jobs·프리셋 배치 공통):

```text
<workspace>/ImageGallery/
  output/<topic>/<filename>.png
  metadata/<topic>/<filename>.png.json
```

기록 JSON에는 실제 프롬프트, 요청한 정확한 `image_model`, 크기, 품질, 작업 종류, 참조 경로, 결과 경로가 들어갑니다. 제공자가 모델 ID를 응답하면 `response_model`도 같은 JSON에 기록합니다. 응답에 없으면 내부 변형 모델은 확인되지 않은 상태입니다. 원본 응답·중복 프롬프트 파일·실행 로그는 기본 생성하지 않습니다. `topic_root` 생략 시 호출한 현재 폴더가 작업 공간입니다. 에이전트는 공용 도구를 실행할 때 사용자의 목적지 프로젝트/작업 공간을 명시해야 합니다. 새 프로젝트를 만들라고 요청한 경우 `<새 프로젝트>/ImageGallery/`가 저장 위치이며 부모 작업 공간의 갤러리를 사용하지 않습니다.

```json
{"mode":"single","prompt":"귀여운 고양이","topic":"cute-cat","topic_root":"E:/workspaces/test"}
```

`tools/ai_image.py`에서도 `concurrency`는 같은 의미입니다(기본 30, 최대 32). `mode=single`에서 `count=50`이면 50장을 만들되 동시에 최대 30장씩 생성합니다. 결과 JSON의 `concurrency`는 실제 사용한 동시 수, `retried`는 자동 재시도 횟수입니다. `mode=jobs`에서는 job 배열을 최대 N개씩 병렬 처리하고, `mode=batch`에서는 `scripts/gen_batch.py --concurrency N`으로 그대로 전달합니다.

## 새 데이터셋 preset 만들기

`presets/<dataset-name>.json` 파일 하나만 추가합니다.

```json
{
  "topic": "my-dataset",
  "size": "1920x1080",
  "quality": "high",
  "model": "gpt-5.5",
  "image_model": "gpt-image-2.5",
  "global_style": "전 이미지에 공통으로 붙는 스타일 문장",
  "negative": "전 이미지에 공통으로 붙는 금지 항목",
  "categories": [
    {
      "name": "category_a",
      "weight": 2,
      "templates": [
        "프롬프트 변형 1",
        "프롬프트 변형 2"
      ]
    },
    {
      "name": "category_b",
      "weight": 1,
      "templates": ["프롬프트 변형 3"]
    }
  ]
}
```

생성 방식:

1. 카테고리를 `weight` 비례로 선택합니다.
2. 선택된 카테고리의 `templates` 중 하나를 고릅니다.
3. `global_style`과 `negative`를 합쳐 최종 prompt를 만듭니다.

## T2IN 공항 샘플 재생성

기존 공항 샘플 작업 입력은 아래 저장소에 있습니다.

```text
E:\CityAI\t2in\t2in-ai-imagesample-generation
```

확인할 파일:

```text
runs/airport-topview-1920x1080/prompts.jsonl
runs/airport-topview-hazards-1920x1080/prompts.jsonl
scripts/generate_airport_dataset.py
```

한 장만 재생성할 때는 `prompts.jsonl`에서 원하는 row의 `prompt`, `reference_images`, `output_path`를 확인한 뒤 `scripts/gen_image.py`로 실행합니다.

예:

```powershell
$row = Get-Content "E:\CityAI\t2in\t2in-ai-imagesample-generation\runs\airport-topview-1920x1080\prompts.jsonl" `
  -TotalCount 1 | ConvertFrom-Json

$argsList = @(
  "scripts\gen_image.py",
  $row.prompt,
  "-o", "E:\ai-image-generator\ImageGallery\output\airport-topview-t2in-normal\체크인카운터\airport_topview_000001.png",
  "--model", "gpt-5.4",
  "--image-model", "gpt-image-2",
  "--size", $row.size,
  "--quality", $row.quality,
  "--action", "generate",
  "--events", "E:\ai-image-generator\ImageGallery\metadata\airport-topview-t2in-single\airport_topview_000001.sse"
)

foreach ($ref in $row.reference_images) {
  $argsList += @("--reference-image", $ref)
}

python @argsList
```

## 에이전트 스킬 설치 위치

선택한 에이전트의 스킬 루트에 정본 폴더 링크를 설치합니다. `~`는 각 PC의 사용자 홈입니다.

```text
Codex:      ~/.agents/skills/ai-image-generator
Claude:     ~/.claude/skills/ai-image-generator
OMP:        ~/.omp/agent/skills/ai-image-generator
```

`py -3 scripts/setup.py --agent codex`처럼 설치합니다. 기존 `~/.codex/skills` 호환 경로, 커스텀 루트, 링크 점검과 업데이트는 [INSTALL.md](INSTALL.md)에 있습니다. 에이전트에게 요청할 때:

```text
Use $ai-image-generator to create one airport top-view image from the T2IN reference prompt.
```

## 운영 주의사항

- 생성 중단 요청을 받으면 프로세스만 중단하고 기존 산출물은 삭제하지 않습니다.
- 대량 생성은 사용자가 명시한 개수만큼만 실행합니다.
- 실패 재시도는 `--resume`을 사용해 이미 만들어진 PNG를 건너뜁니다.
- 동시 수는 계정 수가 상한이 아닙니다. 계정 하나로 동시 30장까지 실패·지연 없이 확인했습니다(2026-10-18). 계정 사용 한도에 걸리면 CLIProxyAPI가 다음 계정으로 넘기고, 클라이언트는 429를 자동 재시도합니다. 재시도 후에도 실패가 반복될 때만 동시 수를 낮춥니다.
- API key는 환경변수 또는 웹 스튜디오의 서버 세션에만 두고 파일에 커밋하지 않습니다.
