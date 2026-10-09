# ai-image-generator

CLIProxyAPI 기반 이미지 생성 도구입니다. 로컬 웹 Studio, 에이전트용 CLI/스킬, 선택적 MCP 서버를 제공합니다.

## 다른 PC에 설치하기

Git과 Python 3.11+가 있는 Windows PowerShell에서:

```powershell
git clone https://github.com/JSCOP/ai-image-generator.git
cd ai-image-generator
py -3 scripts/setup.py --agent codex
py -3 scripts/setup.py --agent codex --check
.\open-image-studio.cmd
```

Claude Code는 `--agent claude`, 둘 다 쓰면 `--agent codex --agent claude`, 웹/CLI만 쓰면 `--agent`를 생략합니다. MCP 의존성은 `--mcp`를 추가합니다. 설치 경로는 자유이며 스킬은 실제 clone을 가리키는 링크로 등록됩니다.

**[INSTALL.md — 프록시 연결, 스킬 설치·업데이트, MCP, 오류 해결, AI에게 줄 설치 요청문](INSTALL.md)** 을 따라 설정하세요. 실제 생성에는 별도의 CLIProxyAPI 서버 주소·키·제공자 인증/크레딧이 필요합니다. 설치 검증은 이미지 생성 API를 호출하지 않습니다.

## MCP 서버

생성/편집/배치·이미지 조회/가공 기능은 `tools/mcp_server.py`의 표준 MCP stdio 서버로 사용할 수 있습니다. 수동 브라우저 UI와 바로가기 관리는 별도입니다. 기존 환경을 유지한 채 MCP 의존성을 추가합니다.

```powershell
py -3 scripts/setup.py --mcp
.\.venv\Scripts\python.exe tools\mcp_server.py --help
```

클라이언트 설정 예시는 [`mcp.json.example`](mcp.json.example)이며 현재 PC의 clone/.venv 절대 경로로 바꿉니다. 클라이언트가 stdio 서버를 실행합니다. 실행 시 `CLIPROXY_BASE_URL`과 `CLIPROXY_API_KEY` 환경변수를 읽습니다. 기본 파일 접근은 저장소 내부이며, 출력 프로젝트나 참조 이미지가 다른 위치에 있으면 `--allow-root "D:\refs"`를 추가합니다. 한 state 디렉터리에는 서버 하나만 실행합니다.

MCP 사용 순서는 `plan_generation` → `start_generation` → `get_job`이며, 계획 단계에서는 API를 호출하지 않습니다. `single`, 서로 다른 `jobs`, preset `batch`를 지원하고, `cancel_job`, `resume_job`, `image_preview`, `crop_image`, `resize_image`, `contact_sheet`, 프리셋·Studio 기록 조회도 제공합니다. 결과는 출력 파일의 존재와 요청 크기를 확인한 뒤 성공으로 보고합니다. API 키는 MCP 응답·상태 파일·로그에 저장하지 않습니다. 자세한 구현 범위와 검증은 [`docs/MCP_IMPLEMENTATION.md`](docs/MCP_IMPLEMENTATION.md)에 있습니다.

기존 머신 토폴로지, 스킬 배포(링크) 규칙, 검증된 이미지 모델 상태는 `docs/SSOT.md`가 정본입니다. 신규 PC 설치는 `INSTALL.md`를 따릅니다. 웹 화면에는 에이전트 설치가 필요하지 않습니다.

## 로컬 이미지 스튜디오 — 더블클릭 실행

Windows에서 **`open-image-studio.cmd`를 더블클릭**합니다. 처음 실행할 때 이 폴더에 `.venv`와 Pillow를 설치하고, 이후에는 `http://127.0.0.1:8766/`을 엽니다. Python 3.11 이상이 필요하며, `py -3` 또는 `python`을 자동으로 확인합니다.

1. **연결 설정**에 사용 가능한 CLIProxyAPI 주소와 키를 입력하고 **연결 확인**을 누릅니다. 기존 환경변수에 키가 있으면 자동으로 연결을 확인합니다.
2. **모델·해상도·품질**을 고르고 메인 프롬프트 또는 Positive를 입력합니다.
3. 필요하면 Negative와 참조 이미지를 추가한 뒤 **한 장 생성**을 누릅니다.
4. 결과를 미리 보거나 다운로드합니다. 실제 파일과 기록은 `ImageGallery/output/<작업 ID>/ (이미지), ImageGallery/metadata/<작업 ID>/ (기록·참조)`에 보존됩니다.

긴 태그는 **입력칸 넓게 보기**로 작성 영역을 화면 너비까지 확장할 수 있습니다. 다시 접어도 입력 내용과 첨부 파일은 유지됩니다. 결과 카드의 **생성 정보 · 사용한 프롬프트**, 참조 이미지, 상세 안내는 필요할 때 펼쳐 봅니다.

같은 폴더에서 다시 실행하면 기존 서버를 재사용합니다. 새로고침은 진행 상태만 다시 가져오며, 생성 요청을 자동으로 재전송하지 않습니다. 한 번에 한 작업만 실행하므로 생성 버튼 연속 클릭으로 요청이 쌓이지 않습니다.

### 모델·프롬프트·참조 이미지

- **ChatGPT 계열:** `gpt-image-2.5`(최신), `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`, `gpt-image-2`, `gpt-image-1.5`. 참조가 있으면 이미지 편집 API를 사용합니다.
- **Gemini:** `gemini-3.1-flash-image`. 참조 이미지를 포함한 생성이 가능합니다.
- **Grok:** `grok-imagine-image-2.0`, `grok-imagine-image-quality`, `grok-imagine-image`. 현재 생성 경로에서는 참조 이미지를 지원하지 않습니다.
- 카탈로그에 있는 모델과 현재 계정에서 사용 가능한 모델은 다릅니다. **연결 확인**은 실제 `/models` 목록을 표시하며, 목록에 없는 모델은 생성할 수 없습니다. 할당량·크레딧·제공자 상태에 따라 실생성은 실패할 수 있습니다.
- Positive/Negative에는 쉼표 태그나 긴 목록을 그대로 붙여넣을 수 있습니다. 전송 문장은 `메인 프롬프트` + `Positive` + `Avoid: Negative` 순서입니다. 별도 네거티브 API가 아니라 텍스트 지시이며, `0.3::tag`, `(tag:1.2)` 같은 가중치의 적용은 보장하지 않습니다.
- Stable Diffusion 계열의 스텝·가이던스·샘플러 설정은 이 제공자 API에서 지원하지 않으므로 표시하지 않습니다.
- 참조는 PNG/JPEG/WebP, 최대 4장, 장당 10MB, 한 변 8192px·총 5천만 픽셀 이하입니다. 파일 선택·끌어놓기·미리보기·개별 제거를 지원합니다. 모델 변경 시 참조를 몰래 제거하지 않으며, Grok과 참조를 함께 선택하면 생성을 막습니다.
- 해상도는 1024×1024, 1536×1024, 1024×1536, 1920×1080, 1080×1920, 2048×2048, 3840×2160 중 선택합니다. **최종 저장 크기**이며, 네이티브 출력에 필요한 중앙 크롭·리샘플링이 적용됩니다. 품질 옵션은 제공자마다 해석이 다릅니다.

### 바탕화면·시작프로그램·종료

프로젝트 폴더에서 필요한 명령만 한 번 실행합니다. 자동 등록이나 관리자 권한 요구는 없습니다.

```powershell
.\open-image-studio.cmd -ShortcutAction Desktop       # 바탕화면 바로가기
.\open-image-studio.cmd -ShortcutAction Startup       # 현재 사용자 로그인 시 자동 시작
.\open-image-studio.cmd -ShortcutAction RemoveStartup # 이 폴더의 자동 시작만 해제
.\open-image-studio.cmd -Stop                         # 실행 중인 이 폴더의 서버 종료
```

로그인 자동 시작은 브라우저를 띄우지 않고 서버만 숨김 실행합니다. 이후 바탕화면 바로가기나 `open-image-studio.cmd`로 화면을 엽니다. **연결 설정 → 서버 종료** 또는 일반 실행 콘솔의 `Ctrl+C`로도 종료할 수 있습니다. 생성 중에는 화면/`-Stop` 종료가 거부되며, 자동 시작 해제는 이미 실행 중인 서버를 멈추지 않습니다.

포트 충돌 시 `.\open-image-studio.cmd -Port 8776`처럼 다른 포트를 지정합니다. 바로가기 등록·종료에도 같은 `-Port`를 사용하세요. 폴더를 옮기기 전 기존 자동 시작을 해제하고 새 위치에서 다시 등록합니다.

### 다른 PC에서 clone해서 사용

```powershell
git clone https://github.com/JSCOP/ai-image-generator.git
cd ai-image-generator
.\open-image-studio.cmd
```

- 각 PC에 Python 3.11 이상과 접근 가능한 **CLIProxyAPI 서버·제공자 인증/할당량**이 필요합니다. 이 저장소가 프록시나 제공자 계정을 자동으로 설치·복제하지는 않습니다.
- 프록시가 같은 PC에 없다면 연결 설정에서 실제 프록시 주소를 입력합니다. 원격 연결은 신뢰할 수 있는 HTTPS 또는 Tailscale 같은 사설망을 사용하세요. ChatGPT/Gemini 웹 구독만으로 API 사용 권한이 자동 제공되는 것은 아닙니다.
- 주소만 `config/image-studio.local.json`에 저장합니다. 주소 우선순위는 이 로컬 설정 → 프로세스/Windows 사용자 `CLIPROXY_BASE_URL` → 기존 머신별 기본값(`docs/SSOT.md`)입니다. 새 PC에서는 실제 주소를 명시하세요.
- 화면에 입력한 키는 **서버 메모리에서만** 유지되고 서버 종료 시 사라집니다. 매번 입력하지 않으려면 Windows 사용자 환경변수 `CLIPROXY_API_KEY`를 설정한 뒤 실행합니다. 키를 코드·명령 기록·Git에 넣지 마세요.
- 초안의 텍스트·모델 설정은 해당 브라우저에 저장됩니다. 결과·작업 프롬프트·참조 원본은 해당 PC의 `ImageGallery/`에 저장되며 Git에 포함되지 않습니다. 기록의 **이 설정 다시 쓰기**는 텍스트 설정만 가져오므로 참조 파일은 다시 첨부합니다.
- `.venv`, 로컬 연결 설정, 결과, 참조 원본, 키는 clone에 포함되지 않습니다. 에이전트 스킬 설정·Node.js·Electron 빌드는 필요 없습니다.
- 서버는 `127.0.0.1`에만 바인딩합니다. 로컬 단일 사용자용이며 외부 공개 웹 서비스로 배포하지 마세요.

Python으로 직접 실행할 수도 있습니다(Windows 외 환경의 수동 실행 경로).

```sh
python -m pip install -r requirements.txt
python tools/image_studio.py --open
```

### 웹 스튜디오 검증

```powershell
.\.venv\Scripts\python.exe -m unittest discover -s tests -v
```

로컬 HTTP 경계, 참조 입력 거부, 세션 연결 설정을 통한 실제 생성 CLI 실행, 결과 다운로드, 오류의 키 비노출을 검증합니다. 테스트는 로컬 제공자 fixture를 사용하며 외부 이미지 생성 비용을 발생시키지 않습니다.

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
- 대량/참조-heavy 작업에서 안정성이 필요하면 품질을 낮추지 말고 `concurrency`를 낮춰 조절합니다. 보통 `concurrency: 2`부터 시작합니다.
- `job_timeout_sec`는 속도 옵션이 아니라 느린 job을 언제 중단할지 정하는 제한입니다.
- `concurrency`를 높인다고 항상 빨라지지 않습니다. 4장 이상 고해상도/참조-heavy 작업은 `2`부터 확인하고, 백엔드가 안정적일 때만 `4` 이상으로 올립니다.
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

## 대화형 이미지 생성 메뉴

```powershell
pwsh -NoProfile -File tools\Image-Menu.ps1
```

단일/다회 생성과 preset 배치 모두 최신 GPT 모델 `gpt-image-2.5`(flare/sunburst 변형 포함), `gpt-image-2`, `gemini-3.1-flash-image`, Grok 이미지 3개 또는 직접 입력한 모델 ID를 선택할 수 있습니다. 기본 이미지 모델은 `gpt-image-2.5`입니다.

## 수동 crop 지정 툴

브라우저에서 이미지 위에 직접 crop 박스를 그리고, crop PNG는 output에, 좌표 JSON은 metadata에 저장할 수 있습니다.

빠른 실행:

```powershell
.\open-manual-crop-tool.ps1
```

또는 더블클릭/명령 프롬프트용:

```cmd
open-manual-crop-tool.cmd
```

기본 실행은 T2IN 체크인카운터 wide crop 폴더가 있으면 `ck-*-wide-crop.png`만 열고, 없으면 `refs/`를 엽니다.

다른 폴더를 열 때:

```powershell
.\open-manual-crop-tool.ps1 `
  -Images "E:\path\to\images" `
  -Output "E:\path\to\manual-crops" `
  -Pattern "*.png"
```

저장 버튼을 누르면 `manual-crop-boxes.json`, crop PNG와 좌표 기록이 생성됩니다. 확대는 --upscale로 요청한 경우에만 만듭니다.

전체 도면 한 장에서 A/B/C처럼 여러 이름 있는 구역을 직접 그릴 때:

```powershell
.\open-manual-region-crop-tool.ps1 `
  -Images "D:\Eagle\CityAI.library\images\MQEVG3G8GDG8U.info\Clipboard - 2026-06-15 16.08.16.png" `
  -Output "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\output\checkin-counter-manual-region-crops-v1\manual-region-crops" `
  -Boxes "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\metadata\checkin-counter-manual-region-crops-v1\manual-region-crop-boxes.json" `
  -Regions "A,B,C,D,E,F,G,H,J,K,L,M,N"
```

브라우저에서 region 버튼을 선택하고 박스를 그린 뒤 `Save JSON + Crops`를 누르면, 구역별 crop PNG와 좌표 JSON이 저장됩니다.

단일 이미지:

```powershell
@'
{
  "mode": "single",
  "prompt": "Modern airport terminal strict vertical top-view scene, clean reflective floors, generic signage, no real logos, no watermark.",
  "topic": "airport-topview-test",
  "count": 1,
  "size": "1920x1080",
  "quality": "high"
}
'@ | python tools\ai_image.py
```

참조 이미지 포함:

```powershell
@'
{
  "mode": "single",
  "prompt": "Create a similar strict vertical airport top-view image using the reference for style only. No real logos, no readable brand signage, no watermark.",
  "topic": "airport-reference-test",
  "count": 1,
  "size": "1920x1080",
  "quality": "high",
  "action": "edit",
  "resume": true,
  "reference_images": [
    "D:/Eagle/CityAI.library/images/MOGQ97HSI25NC.info/Clipboard - 2026-04-27 13.59.04.png"
  ]
}
'@ | python tools\ai_image.py
```

`action`은 `auto`, `generate`, `edit` 중 하나입니다. GPT 참조 이미지의 구조를 유지하며 재스타일링할 때는 `edit`를 명시합니다. `single`과 `jobs` 모두 top-level 기본값과 job별 override를 지원합니다.

도면 구조가 최우선이면 스타일 참조 이미지를 함께 전달하지 않는 것이 안전합니다. 구조 원본 한 장만 `reference_images`에 넣고 색상, 재질, 시설 표현, top-view 규칙을 prompt에 서술합니다. 여러 참조 이미지를 함께 넣으면 모델이 스타일 이미지의 공간 배치를 섞을 수 있습니다.

중단된 반복 생성을 재개할 때는 기존 topic/count를 유지하고 `resume: true`를 지정합니다. 이미 존재하는 numbered output은 `image_skipped`로 건너뛰고 누락된 파일만 생성합니다.

응답 예:

```json
{
  "ok": true,
  "mode": "single",
  "outputs": ["E:\\ai-image-generator\\ImageGallery\\output\\airport-topview-test\\airport-topview-test.png"],
  "failures": [],
  "output_dir": "E:\\ai-image-generator\\ImageGallery\\output\\airport-topview-test"
}
```

이미지 provider/model 선택:

- `gpt-image-2.5`(기본), `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`, `gpt-image-2`, `gpt-image-1.5`: OpenAI `/v1/images/generations` 경로. 참조 이미지가 있으면 `/v1/images/edits` 경로
- `gemini-3.1-flash-image`: Gemini native `generateContent` 경로; Antigravity OAuth 사용 가능
- `grok-imagine-image-2.0`, `grok-imagine-image-quality`, `grok-imagine-image`: xAI `/v1/images/generations` 경로

`tools/ai_image.py` JSON의 `image_model` 또는 직접 CLI의 `--image-model`로 모델을 지정합니다. 직접 설정한 `CLIPROXY_IMAGE_MODEL` 환경변수도 CLI 기본 설정으로 유지되며 요청의 명시 모델이 우선합니다. 생략 시 `gpt-image-2.5`를 사용합니다. `gpt-image-2.5` 등 `gpt-image-*` 참조 편집은 `reference_images`와 `action: "edit"`를 함께 지정합니다. Gemini는 참조 이미지를 native inline data로 전달합니다. Grok 3개 모델은 text-to-image만 지원하며 `reference_images`가 있으면 API 호출 전에 거부합니다.

Gemini native 해상도는 요청한 `size`의 긴 변을 기준으로 `512`, `1K`, `2K`, `4K` 버킷을 자동 선택합니다. 16:9 실측 출력은 각각 약 `688x384`, `1376x768`, `2752x1536`, `5504x3072`이며 `8K`는 지원되지 않습니다. 모델 출력은 요청 픽셀과 정확히 일치하지 않을 수 있습니다.

CLIProxyAPI의 Grok 이미지 노출 설정 예시는 `config/cliproxy-image-providers.example.yaml`에 있습니다. 최종 파일은 provider 원본 크기와 관계없이 `size`에 지정한 정확한 픽셀 크기로 저장됩니다.

```json
{
  "mode": "single",
  "prompt": "A clean green triangle on white",
  "topic": "gemini-provider-test",
  "image_model": "gemini-3.1-flash-image",
  "size": "1024x1024",
  "quality": "low"
}
```

Grok 2.0 예시:

```json
{
  "mode": "single",
  "prompt": "A clean green triangle on white, no text or watermark",
  "topic": "grok-provider-test",
  "image_model": "grok-imagine-image-2.0",
  "size": "1024x1024",
  "quality": "low"
}
```

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
python scripts\gen_batch.py presets\extraction-rpg.json --count 10 --concurrency 4

# 이어서 생성
python scripts\gen_batch.py presets\extraction-rpg.json --count 100 --resume --concurrency 8
```

배치 옵션:

- `--count N`: 생성 개수
- `--concurrency N`: 동시 워커 수. 여러 이미지를 동시에 요청합니다. `N=4`면 한 번에 최대 4개의 `gen_image.py` 프로세스가 선택한 provider endpoint를 호출합니다.
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

기존 자료 정리: `python tools/organize_gallery.py --workspace <workspace>`로 계획을 확인한 뒤 `--apply`를 붙입니다. 이미지는 output, 기록과 그 밖의 기존 파일은 metadata로 이동하며 이름 충돌 시 별도 이름으로 보존합니다. 빈 원본 폴더만 제거합니다.

`tools/ai_image.py`에서도 `concurrency`는 같은 의미입니다. `mode=single`에서 `count=8, concurrency=4`면 8장을 만들되 동시에 최대 4장만 생성합니다. `mode=jobs`에서는 job 배열을 최대 N개씩 병렬 처리하고, `mode=batch`에서는 `scripts/gen_batch.py --concurrency N`으로 그대로 전달합니다.

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
- `--concurrency`를 너무 높이면 로컬 CLIProxyAPI 계정 수, upstream rate limit, CPU/메모리, 네트워크 대기 때문에 실패율이 올라갈 수 있습니다. 안정성 우선이면 3~4, 빠른 대량 생성이면 6~8부터 확인합니다.
- API key는 환경변수 또는 웹 스튜디오의 서버 세션에만 두고 파일에 커밋하지 않습니다.
