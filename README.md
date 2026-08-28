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
- `grok-imagine-image` 또는 `grok-imagine-image-quality`: xAI `/v1/images/generations` 경로

`tools/ai_image.py` JSON에 `image_model`을 지정합니다. Gemini는 참조 이미지를 native inline data로 전달합니다. Grok 참조 이미지 편집은 현재 CLI에서 지원하지 않습니다.

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

## 이미지 생성 CLI 사용

항상 `tools/ai_image.py`를 사용합니다. 명령은 이 저장소 루트에서 실행하되, `workspace_root`에는 현재 대화가 작업 중인 폴더를 지정합니다.

```powershell
$spec = @'
{
  "mode": "single",
  "prompt": "고양이가 우주를 유영하는 시네마틱 사진",
  "topic": "cat-in-space",
  "workspace_root": "E:/abc",
  "date": "2026-07-13",
  "count": 1,
  "size": "1920x1080",
  "quality": "high"
}
'@
python tools\ai_image.py --json $spec
```

최종 이미지 저장 구조:

```text
<workspace_root>/ImageGallery/<YYYY-MM-DD>/<topic>/*.png
```

제목 폴더에는 이미지만 저장됩니다. 프롬프트, 요청 설정, 참조 이미지 등 재현용 입력은 생성기 저장소 안에 별도로 보관됩니다.

```text
C:/Users/jisung/workspaces/ai-image-generator/inputs/<YYYY-MM-DD>/<topic>/
  request.json        # single/jobs 요청
  request.jsonl       # batch 요청
  references/*        # 복사된 참조 입력
```

주요 필드:

- `workspace_root`: 현재 대화/작업 폴더
- `date`: 날짜 폴더 `YYYY-MM-DD`; 생략 시 로컬 오늘 날짜
- `topic`: 이미지 생성 제목 및 제목 폴더명
- `resume`: 기존 이미지를 덮어쓰지 않고 누락된 이미지만 생성
- `reference_images`: 참조 이미지 경로; 실행 전에 중앙 입력 폴더로 복사됨
- `concurrency`: 동시 생성 수

`mode: "batch"`는 먼저 `dry_run: true`로 계획을 기록하고 확인한 뒤 실제 실행합니다. 기존 제목 폴더에 파일이 있으면 기본적으로 중단되며, 이어서 생성할 때만 `resume: true`를 사용합니다.
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

## 저장된 입력으로 재생성

재현용 요청은 생성기 저장소의 날짜/제목 폴더에 있습니다.

```text
C:/Users/jisung/workspaces/ai-image-generator/inputs/<YYYY-MM-DD>/<topic>/request.json
C:/Users/jisung/workspaces/ai-image-generator/inputs/<YYYY-MM-DD>/<topic>/request.jsonl
```

단일 및 jobs 요청은 `request.json`을 확인하고 동일한 `workspace_root`, `date`, `topic`에 `resume: true`를 추가해 `tools/ai_image.py`로 실행합니다. Batch 요청은 `request.jsonl`의 prompt와 output 경로를 기준으로 누락된 항목만 재생성합니다. `scripts/gen_image.py`를 직접 호출하거나 이벤트·로그 파일을 별도로 남기지 않습니다.

최종 제목 폴더에는 이미지 외 파일이 없어야 합니다.

## Oh My Pi 스킬 설치 위치

프로젝트 스킬과 Oh My Pi 설치 스킬을 동일하게 유지합니다.

```text
프로젝트: C:/Users/jisung/workspaces/ai-image-generator/skills/ai-image-generator/SKILL.md
Oh My Pi: C:/Users/jisung/.pi/agent/skills/ai-image-generator/SKILL.md
```
## 운영 주의사항

- 생성 중단 요청을 받으면 프로세스만 중단하고 기존 산출물은 삭제하지 않습니다.
- 대량 생성은 사용자가 명시한 개수만큼만 실행합니다.
- 실패 재시도는 `--resume`을 사용해 이미 만들어진 PNG를 건너뜁니다.
- `--concurrency`를 너무 높이면 로컬 CLIProxyAPI 계정 수, upstream rate limit, CPU/메모리, 네트워크 대기 때문에 실패율이 올라갈 수 있습니다. 안정성 우선이면 3~4, 빠른 대량 생성이면 6~8부터 확인합니다.
- API key는 환경 변수로만 설정하고 파일에 커밋하지 않습니다.
