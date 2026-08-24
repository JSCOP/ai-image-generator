# ai-image-generator

CLIProxyAPI 기반 이미지 생성 CLI입니다. Claude Code, Codex, Hermes Agent가 같은 방식으로 사용할 수 있도록 단일 이미지, JSON 입력, preset 배치 생성 흐름을 제공합니다.

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

## 대화형 이미지 생성 메뉴

```powershell
pwsh -NoProfile -File tools\Image-Menu.ps1
```

단일/다회 생성과 preset 배치 모두 `gpt-image-2`, `gemini-3.1-flash-image`, Grok 이미지 3개 또는 직접 입력한 모델 ID를 선택할 수 있습니다. Grok 선택 시 `grok-imagine-image-2.0`을 권장합니다.

## 수동 crop 지정 툴

브라우저에서 이미지 위에 직접 crop 박스를 그리고, 좌표 JSON과 crop PNG를 저장할 수 있습니다.

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

저장 버튼을 누르면 `manual-crop-boxes.json`, crop PNG, 2x deterministic upscale PNG, contact sheet가 함께 생성됩니다.

전체 도면 한 장에서 A/B/C처럼 여러 이름 있는 구역을 직접 그릴 때:

```powershell
.\open-manual-region-crop-tool.ps1 `
  -Images "D:\Eagle\CityAI.library\images\MQEVG3G8GDG8U.info\Clipboard - 2026-06-15 16.08.16.png" `
  -Output "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\checkin-counter-manual-region-crops-v1\output\manual-region-crops" `
  -Boxes "E:\CityAI\IncheonProject\t2in-dev\ImageGallery\checkin-counter-manual-region-crops-v1\manual-region-crop-boxes.json" `
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
  "outputs": ["E:\\ai-image-generator\\output\\airport-topview-test\\airport-topview-test.png"],
  "failures": [],
  "output_dir": "E:\\ai-image-generator\\output\\airport-topview-test"
}
```

이미지 provider/model 선택:

- `gpt-image-2` 또는 `gpt-image-1.5`: OpenAI Responses 이미지 도구 경로
- `gemini-3.1-flash-image`: Gemini native `generateContent` 경로; Antigravity OAuth 사용 가능
- `grok-imagine-image-2.0`(권장), `grok-imagine-image-quality`, `grok-imagine-image`: xAI `/v1/images/generations` 경로

`tools/ai_image.py` JSON에 `image_model`을 지정합니다. Gemini는 참조 이미지를 native inline data로 전달합니다. Grok 3개 모델은 text-to-image만 지원하며 `reference_images`가 있으면 API 호출 전에 거부합니다.

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
output/<topic>/<filename>.png
```

주요 옵션:

- `--topic <name>`: `output/<topic>` 폴더 이름. 생략 시 prompt에서 자동 추출
- `-o <file>`: 출력 파일명 또는 경로
- `--size 1920x1080`: 최종 결과 파일의 정확한 크기
- `--quality low|medium|high`: 품질. 생략 시 `high`
- `--image-model <id>`: 이미지 provider 모델. Grok 권장값은 `grok-imagine-image-2.0`
- `--reference-image <path>`: 참조 이미지. 여러 번 지정 가능
- `--events <path>`: raw SSE 응답 저장

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
- `--image-model <id>`: preset의 `image_model` 덮어쓰기
- `--topic-root <dir>`: `output/`, `runs/`가 만들어질 부모 경로
- `--resume`: 이미 존재하는 PNG는 건너뜀
- `--dry-run`: `prompts.jsonl` 계획만 기록
- `--max-failures N`: 누적 실패 시 중단

출력 구조:

```text
output/
  <topic>/
    <category>/<topic>_NNNNNN.png
runs/
  <topic>/<YYYYMMDD_HHMMSS>/
    prompts.jsonl
    metadata.jsonl
    failures.jsonl
    events/*.sse
```

`tools/ai_image.py`에서도 `concurrency`는 같은 의미입니다. `mode=single`에서 `count=8, concurrency=4`면 8장을 만들되 동시에 최대 4장만 생성합니다. `mode=jobs`에서는 job 배열을 최대 N개씩 병렬 처리하고, `mode=batch`에서는 `scripts/gen_batch.py --concurrency N`으로 그대로 전달합니다.

## 새 데이터셋 preset 만들기

`presets/<dataset-name>.json` 파일 하나만 추가합니다.

```json
{
  "topic": "my-dataset",
  "size": "1920x1080",
  "quality": "high",
  "model": "gpt-5.5",
  "image_model": "gpt-image-2",
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
  "-o", "E:\ai-image-generator\output\airport-topview-t2in-normal\체크인카운터\airport_topview_000001.png",
  "--model", "gpt-5.4",
  "--image-model", "gpt-image-2",
  "--size", $row.size,
  "--quality", $row.quality,
  "--action", "generate",
  "--events", "E:\ai-image-generator\runs\airport-topview-t2in-single\airport_topview_000001.sse"
)

foreach ($ref in $row.reference_images) {
  $argsList += @("--reference-image", $ref)
}

python @argsList
```

## 에이전트 스킬 설치 위치

같은 `ai-image-generator` 스킬을 아래 위치에 설치합니다.

```text
Codex:      C:\Users\js\.codex\skills\ai-image-generator
Claude:     C:\Users\js\.claude\skills\ai-image-generator
Hermes:     C:\Users\js\.hermes\skills\media\ai-image-generator
```

에이전트에게 요청할 때:

```text
Use $ai-image-generator to create one airport top-view image from the T2IN reference prompt.
```

## 운영 주의사항

- 생성 중단 요청을 받으면 프로세스만 중단하고 기존 산출물은 삭제하지 않습니다.
- 대량 생성은 사용자가 명시한 개수만큼만 실행합니다.
- 실패 재시도는 `--resume`을 사용해 이미 만들어진 PNG를 건너뜁니다.
- `--concurrency`를 너무 높이면 로컬 CLIProxyAPI 계정 수, upstream rate limit, CPU/메모리, 네트워크 대기 때문에 실패율이 올라갈 수 있습니다. 안정성 우선이면 3~4, 빠른 대량 생성이면 6~8부터 확인합니다.
- API key는 환경 변수로만 설정하고 파일에 커밋하지 않습니다.
