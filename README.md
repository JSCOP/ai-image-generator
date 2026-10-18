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
