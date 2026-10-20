# Image Studio 앱 (Rust + Tauri) 설계

- 작성일: 2026-10-18
- 상태: 승인 (2026-10-18, 사용자 "계획을 잘 세워서 작업을 진행해줘"). 외부 검토 `docs/superpowers/reviews/2026-10-18-image-studio-rust-app-review.md`의 P1 항목을 §5.4, §6, §7에 반영함
- 대상 저장소: `E:\ai-image-generator` (`github.com/JSCOP/ai-image-generator`, 개인 저장소)

표기: **[사용자 결정]**은 대화에서 사용자가 정한 것, **[작성자 판단]**은 설계자가 정한 기본값(사용자가 바꿀 수 있음), **[구현 중 확인]**은 실제 호출로 검증해야 하는 사실이다.

## 1. 목적과 배경

지금 저장소는 AI 에이전트(Claude Code, Codex)가 부르는 Python CLI(`tools/ai_image.py`)와 MCP 서버로 이미지를 생성한다. 사람이 직접 쓰던 웹 Studio·PowerShell 메뉴·크롭 도구는 2026-10-18에 제거했다(커밋 `4389e77`, 제거 직전 상태는 `52e220b`).

새로 만드는 것은 **사람이 직접 쓰는 데스크톱 앱 "Image Studio"**다. 이유:

1. 생성·참조 편집·**마스크 부분 편집**·**투명 배경**·병렬 생성 결과를 한 화면에서 다루고 싶다.
2. **다른 사람에게 설치 파일로 나눠 줄 수 있어야 한다.** [사용자 결정]
3. 받는 사람마다 연결 환경이 다르다: 자기 CLIProxyAPI 프록시, OpenAI/Gemini 등 공식 API 키, 또는 특정 IP(예: Tailscale)로 다른 PC의 프록시를 사용. [사용자 결정]

설치 파일 배포에 Python 런타임을 끼워 넣는 것은 용량·백신 오탐·업데이트 관리 부담이 크다. 그래서 **생성 엔진을 Rust crate로 이전**하고, 앱과 CLI가 같은 엔진을 쓰게 한다. [사용자 결정: "지금 있는 걸 Rust crate로 마이그레이션하면서 구현"]

## 2. 목표 / 비목표 / 반목표

### 목표

- G1. Python 없이 동작하는 Windows 설치 파일(앱) 하나.
- G2. 기존 CLI 동작을 Rust로 이전한 CLI. `ai_image.py`와 **같은 JSON 입력/출력 계약**을 지킨다(§5.2).
- G3. 연결 프로필: CLIProxyAPI, OpenAI 공식, Gemini 공식, xAI 공식, 기타 OpenAI 호환. 여러 개 저장·전환.
- G4. 앱 기능: 생성(다장 병렬), 참조 편집, 마스크 부분 편집, 투명 배경(모델 자체 + 생성 후 배경 제거), 진행·재시도 표시, 갤러리/히스토리.
- G5. 첫 실사용 테스트: **노트북에 설치한 앱 → Tailscale → 이 PC(`DESKTOP-SB818KQ`)의 CLIProxyAPI**로 생성 성공.

### 비목표 (이번 범위 아님)

- 노드/순서도 방식 작업 편집기. 사용자가 불필요하다고 판단했다. [사용자 결정]
- macOS/Linux 설치 파일. 코드는 이식 가능하게 쓰되 배포·검증은 Windows만. [작성자 판단]
- 자동 업데이트, 코드 서명 인증서 구매. 1차 배포는 서명 없는 설치 파일(SmartScreen 경고 안내).
- 데이터셋 전용 스크립트(`scripts/t2in_event_hazards.py`, `scripts/gen_art_candidates.py`)의 Rust 이전.
- Stable Diffusion식 스텝·샘플러 설정(제공자 API에 없음).

### 반목표 (기술적으로 돌아가도 실패로 보는 것)

- API 키가 설치 파일, 설정 파일, 로그, 이미지 기록 JSON, 오류 메시지 어디에든 평문으로 남는 것.
- 요청한 모델이 없을 때 다른 모델로 몰래 바꿔 생성하는 것(기존 원칙 유지).
- 기존 결과 이미지를 덮어쓰는 것(기존 원칙 유지).
- "마스크 밖은 그대로"를 켰는데 마스크 밖 픽셀이 바뀌는 것.
- "투명"이라고 표시했는데 실제 PNG에 알파 채널(투명 정보)이 없는 것.
- 연결 실패 시 원인을 구분하지 못하고 "실패"만 띄우는 것.

## 3. 제약

- 플랫폼: Windows 11. 앱 화면은 Windows 기본 WebView2 사용.
- 개인 저장소 규칙: 커밋 작성자는 JSCOP. 커밋 전 `C:/Users/js/.config/git-identity-guard/POLICY.md`의 점검을 실행한다. 회사(CityAI) 신원이 들어가면 안 된다.
- 비밀값: API 키는 운영체제 비밀 보관소(Windows 자격 증명 관리자)에만 저장한다.
- 기존 저장 규칙 유지: `<작업폴더>/ImageGallery/output/<topic>/`에 이미지만, `metadata/<topic>/<파일명>.json`에 기록.
- 이전 기간 동안 기존 Python CLI·MCP·스킬은 계속 동작해야 한다(에이전트 작업이 끊기면 안 됨).

## 4. 전체 구조

```
ai-image-generator/
  Cargo.toml                 # Rust workspace
  crates/
    image-core/              # 엔진: 제공자 호출, 크기 맞춤, 저장, 병렬·재시도, 마스크 합성, 알파 검사
    image-cli/               # ai_image.py와 같은 JSON 계약의 명령줄 프로그램
  apps/
    studio/                  # Tauri 2 앱 (src-tauri: Rust, src: 화면)
  scripts/, tools/           # 기존 Python (이전 완료 후 제거, §10 단계 6)
```

[작성자 판단] 디렉터리 이름과 workspace 배치는 구현자가 바꿀 수 있다. 핵심은 **엔진 1개를 앱·CLI가 공유**하는 것.

- 화면: Tauri 2 + TypeScript + Svelte + Vite. [작성자 판단, 사용자는 Tauri 방향 승인] 구현자가 React가 명확히 낫다고 판단하면 바꿔도 된다.
- Rust 쪽 앱 코드(`src-tauri`)는 얇게: 화면 요청을 `image-core` 호출로 바꾸고 진행 이벤트를 화면에 보낸다.
- 비동기 실행: `tokio`. HTTP: `reqwest`(rustls). 이미지: `image` crate. 직렬화: `serde`/`serde_json`. 비밀 보관: `keyring` crate.

## 5. 엔진 (`image-core`)과 CLI (`image-cli`)

### 5.1 이전 대상과 동작 기준

기존 Python 코드가 **동작 명세서**다. 다음 파일의 동작을 이전한다(합계 약 1,850줄):

| Python | 옮길 동작 |
|---|---|
| `scripts/gen_image.py` | 제공자별 요청 형식(아래 §6), base URL 해석(`resolve_base_url`), API 키 해석, 응답에서 이미지 추출, 정확한 크기 맞춤(§7.1의 맞춤 방식으로 확장), 기록 JSON 쓰기, 일시적 실패 구분(종료 코드 3) |
| `tools/ai_image.py` | JSON spec 검증, single/jobs/batch 모드, 병렬 실행(기본 30, 1..32), 재시도(기본 2, 0..5, 지수 백오프; 408/425/429/5xx·연결 끊김·타임아웃), resume, dry_run, 작업별 타임아웃, 진행 이벤트(JSON 줄, stderr), `--list-models` |
| `scripts/gen_batch.py` | preset JSON으로 batch 계획·실행, seed 고정 |
| `scripts/gallery.py` | `ImageGallery/output|metadata/<topic>` 경로 규칙, topic slug |
| `tools/image_core.py` | 모델 카탈로그, 연결 확인(리다이렉트 미추종), 프롬프트 합성(`Avoid:` 줄) |

이전하지 않는 것 [작성자 판단]: `gen_image.py`의 **Responses API 경로**(`POST /v1/responses` + `image_generation` 도구, gpt-image/gemini/grok 외 모델용 옛 대체 경로). 현재 스킬·카탈로그의 모든 모델은 Images API 또는 Gemini native를 쓰므로 제거한다. `model` spec 키는 받아도 무시하고 경고만 낸다.

### 5.2 CLI 계약

- 입력: `--spec <file>`, `--json <string>`, 표준입력, `--schema`, `--example`, `--list-models`. 키 이름·기본값·범위는 `ai_image.py --schema` 출력과 같다.
- 출력: 표준출력 마지막 줄에 결과 JSON 한 줄(`ok`, `mode`, `outputs`, `failures`, `topic_dir`, `output_dir`, `metadata_dir`, `elapsed_sec`, `concurrency`, `retried`, `error`, dry_run 시 `planned_outputs`). 진행 이벤트는 표준에러에 JSON 줄.
- 종료 코드: 0 성공, 2 일부 실패, 1 치명적 오류.
- 추가 키(새 기능): `mask`(마스크 PNG 경로), `background`(`"auto"|"opaque"|"transparent"`), `remove_background`(bool), `keep_outside_mask`(bool, 기본 true), `profile`(연결 프로필 이름). 기존 키의 의미는 바꾸지 않는다.
- 연결 해석 순서: `--profile`/spec `profile` → 프로세스 환경변수 `CLIPROXY_BASE_URL`/`CLIPROXY_API_KEY` → Windows 사용자 환경변수 → 앱의 **기본 프로필**(§6.1) → 없으면 오류("연결 설정 필요"). 에이전트 스킬의 기존 동작(환경변수)을 깨지 않는다. [작성자 판단]
- **기존 머신별 하드코딩 기본값은 Rust로 옮기지 않는다.** Python의 `default_base_url`은 호스트 이름이 `DESKTOP-SB818KQ`가 아니면 소유자의 사설 Tailscale 주소(이전 주소)로 보내는데, 배포본에 들어가면 받는 사람 모두가 소유자 PC로 요청하게 된다. 소유자 PC들은 `CLIPROXY_BASE_URL` 사용자 환경변수 또는 프로필로 대체한다(단계 0에서 각 PC의 설정 확인). 동등성 테스트는 이 항목만 의도된 차이로 제외한다. [작성자 판단]

- 새 키는 **받으면 적용하거나, 유료 호출 전에 구조화된 오류로 거부**한다. 조용히 무시하지 않는다(검토 P1-04). 예: 없는 마스크 파일, 투명 미지원 모델 + `background:"transparent"`, `background:"transparent"`와 `remove_background:true` 동시 지정, JPEG + 투명/정확 보존. CLI·MCP 스키마도 같이 갱신한다.
- 추가 키 `fit`(`"cover"|"contain"`, §7.1). 결과 기록에는 실제 적용한 배경 처리 방식, 맞춤 방식, 마스크 합성 여부, 선택 프로필 id, 공급자 원본 크기를 남긴다.
- 종료 코드 주의: 0/2/1은 프로세스의 실제 반환값 기준이다. PowerShell 래퍼가 보고하는 값과 혼동하지 않는다.

### 5.3 동등성 검증

- 기존 Python 테스트(`tests/` 34개 중 엔진 동작 31개, 나머지 3개는 스킬 링크용 `test_sync_skills.py`)가 쓰는 **가짜 제공자(loopback fixture)** 방식을 Rust 통합 테스트로 옮긴다. 실제 생성 비용 없음.
- 같은 fixture에 Python CLI와 Rust CLI를 각각 돌려 **출력 파일 이름·크기·기록 JSON 키·결과 JSON**이 같은지 비교하는 테스트를 이전 기간 동안 유지한다.
- 실제 프록시 대상 확인은 단계별 소수 장(1~2장)만.
- 비교는 같은 로컬 HTTP fixture로 한다. 시간·절대 경로·소요 시간은 정규화하고, **의도된 차이 목록**(하드코딩 주소 제거, Responses 경로 제거, 리다이렉트 차단, 새 키, 완료 상태 기록)을 문서로 유지한다.

### 5.4 엔진 공통 규칙 (1단계에서 GUI 전에 고정, 검토 P1-01·P1-05)

**HTTP 보안**
- 모델 조회·생성·편집 모두 같은 HTTP 클라이언트를 쓴다. 리다이렉트는 **따라가지 않고** 3xx를 오류 `redirect`로 보고한다. 인증 정보는 프로필의 주소에만 붙인다.
- 오류 표시·기록 전에 키와 인증 헤더 값을 제거한다. 공급자 응답 본문은 길이를 제한해 정제 후 보관한다.
- 오류 구조: `category`(network/dns/timeout/tls/auth/permission/quota/rate_limit/conflict/cancelled/not_found/redirect/unsupported/invalid_request/provider/internal), `provider_code`(예: `personal-team-blocked:spending-limit`), `http_status`, `retryable`, `message`. 401/403을 모두 "키 오류"로 묶지 않는다.

**저장·재개 (DB 없이 파일로)**
1. **예약**: 출력 경로를 정한 뒤 `metadata/<topic>/<파일>.json.lock`을 배타 생성(create_new)으로 만든다. 실패하면 다른 작업이 쓰는 중이므로 그 번호는 API 호출 없이 실패(`category:"conflict"`)로 보고한다. 파일 이름은 바꾸지 않는다(결정적 이름 유지).
2. **요청 기록**: 기록 JSON을 `status:"pending"`, `request_id`(UUID), 요청 해시(프롬프트·모델·크기·참조 해시 등)로 원자적 쓰기.
3. **이미지 저장**: 같은 폴더의 임시 파일(`.<파일>.part`)에 쓰고, 다시 디코딩해 크기·알파·해시를 확인한 뒤 rename으로 최종 이름에 공개.
4. **완료 기록**: 기록 JSON을 `status:"complete"`, `sha256`, `width`, `height`로 원자적 갱신 후 lock 삭제. 실패 시 `status:"failed"`와 오류 구조를 남기고 이미지는 만들지 않는다.
- resume은 **기록이 complete + 요청 해시 일치 + 이미지 해시 일치**일 때만 건너뛴다. 해시가 다르거나 손상됐으면 그 번호는 건드리지 않고 오류로 보고한다(덮어쓰기 금지 원칙). 오래된 lock(프로세스 없음)은 정리 후 재시도한다.
- Python이 만든 기존 기록(`status` 없음)은 "이미지 파일이 열리면 완료"로 간주하는 호환 규칙을 둔다.

**합격 fixture**: 다른 호스트/포트로 302 → 목적지에 인증 요청 0건(조회·생성·편집 모두). 같은 경로 동시 시작, 저장 중 강제 종료(.part 잔존), 손상 이미지, 다른 프롬프트의 resume이 결과를 잘못 재사용하지 않음.

## 6. 연결 프로필과 제공자

### 6.1 프로필

| 종류 | 기본 주소 | 인증 | 비고 |
|---|---|---|---|
| CLIProxyAPI | 사용자 입력 (예: `http://127.0.0.1:8317/v1`, Tailscale `http://<프록시 PC의 Tailscale 주소>:8317/v1`) | `Authorization: Bearer <키>` | 현재 검증된 경로. 모든 모델을 한 주소로 |
| OpenAI 공식 | `https://api.openai.com/v1` | Bearer | gpt-image-* |
| Gemini 공식 | `https://generativelanguage.googleapis.com` | `x-goog-api-key` | gemini-*-image. native `/v1beta/models`(페이지 처리) 별도 어댑터 |
| xAI 공식 | `https://api.x.ai/v1` | Bearer | grok-imagine-*. 참조 편집은 JSON `/v1/images/edits` 지원(공식 문서, 검토 W2). 현재 Python의 Grok 참조 차단은 클라이언트 구현 제한일 뿐 |
| OpenAI 호환(기타) | 사용자 입력 | Bearer | 구조는 CLIProxyAPI와 같게 취급 |

- 프로필 목록(키 제외)은 `%APPDATA%\ImageStudio\profiles.json`. 키는 `keyring`으로 자격 증명 관리자에 `ImageStudio/<프로필 id>`로 저장. [작성자 판단]
- CLI도 같은 프로필 파일과 보관소를 읽는다(`--profile`).
- 주소 검증은 기존 `normalize_base_url` 규칙을 따른다(공백·인증정보·쿼리 금지). **`/v1` 자동 보정은 OpenAI형 어댑터(CLIProxyAPI·OpenAI·xAI·호환)에만** 적용하고 Gemini native 주소는 바꾸지 않는다.
- 프로필은 **공급자 종류 + 주소 + 키 참조**를 한 묶음으로 확정한다. 선택한 프로필이 불완전하면 다른 프로필이나 환경변수의 키를 빌리지 않고 오류를 낸다. 환경변수 연결(`CLIPROXY_*`)은 "환경변수" 가상 프로필 하나로 취급해 섞이지 않게 한다.
- "평문 키를 저장하지 않음"은 앱이 새로 받은 키에 대한 약속이다. 기존 에이전트용 `CLIPROXY_API_KEY` 사용자 환경변수는 호환을 위해 읽기만 한다.
- 동시 실행 기본값은 프로필 종류별로 둔다: CLIProxyAPI 30(기존 실측), 공식 API는 보수적 기본값(예: 4)으로 시작하고 사용자가 조절. [작성자 판단]
- **보안 경고**: `http://`이면서 주소가 루프백(127.0.0.0/8, ::1), 사설망(10/8, 172.16/12, 192.168/16), Tailscale(100.64.0.0/10, `*.ts.net`)이 아니면 저장 전에 경고. Tailscale 구간은 WireGuard로 암호화되므로 `http` 허용.

### 6.2 연결 확인

`GET <base>/models`(Gemini 공식은 해당 목록 API)를 리다이렉트 없이 호출한다. 연결 상태는 **서버 접근 → 인증 → 모델 목록 → 실제 생성 가능**의 단계로 따로 표시한다(검토: Grok은 목록에 있어도 지출 한도로 생성 불가). "실제 생성 가능"은 마지막 생성 결과(성공/할당량 오류)로만 갱신하며, 연결 확인 버튼이 유료 생성을 하지 않는다. 결과 구분:

| 상황 | 표시 |
|---|---|
| 이름 해석 실패 | 주소 오타 또는 Tailscale 꺼짐/로그아웃 |
| 연결 거부 | 서버 꺼짐 또는 포트 오류 |
| 시간 초과 | 방화벽 차단, Tailscale 경로 문제, 서버 응답 없음 |
| 401 | 키 오류 |
| 403 / 402 / 429(할당량) | 권한·지출 한도·정책 구분(공급자 코드 표시) |
| 404 | 주소 경로 오류(`/v1` 확인) |
| 3xx | 다른 주소로 이동 요구(따라가지 않음, 키 유출 방지) |
| TLS 오류 | 인증서 문제 |
| 200 | 사용 가능 모델 목록 → 이미지 모델만 걸러 표시 |

### 6.3 모델별 기능표

엔진은 모델 ID와 프로필 종류로 기능을 판정해 화면이 불가능한 조합을 미리 막는다. 표에는 **공식 문서 근거**와 **이 환경 실측(날짜)**을 따로 적는다. 실측(2026-10-18, CLIProxyAPI 8.0.32 경유): GPT/Gemini 생성·참조 편집 성공, sunburst 투명 RGBA 성공, Grok 지출 한도 실패. 공식 API 직접 연결은 미검증.

| 모델 | 참조 | 마스크 | 투명(모델 자체) |
|---|---|---|---|
| gpt-image-2.5-sunburst / -flare | O | O [구현 중 확인] | O (공식 문서) |
| gpt-image-2.5 (접미사 없음) | O | O (CLIProxyAPI 수신 확인, 마스크 준수 여부는 미확정) | [구현 중 확인] 공식 문서에 없음 |
| gpt-image-2 | O | O [구현 중 확인] | 미리보기 지원, 중단 보고 있음 [구현 중 확인] |
| gpt-image-1.5 | O | O | [구현 중 확인] |
| gemini-3.1-flash-image | O | X (프롬프트+합성으로 대체) | X (생성 후 배경 제거로 대체) |
| grok-imagine-* | O(xAI 문서) / CLIProxyAPI 경유는 [구현 중 확인] | X | X (생성 후 배경 제거로 대체) |

### 6.4 요청 형식 차이

- CLIProxyAPI의 GPT 편집은 현재 **JSON 본문 + `images:[{image_url: data URL}]`** 형식으로 동작이 검증됐다(`gen_image.build_openai_images_payload`).
- OpenAI 공식 편집 API 문서에는 **JSON(`images`, `mask` 이미지 참조)과 multipart 예제가 모두 있다**(검토 W1). 공식 프로필도 JSON을 기본으로 쓰고, 실검증에서 거부되면 multipart로 바꾼다. 문서 근거와 실측 여부를 기능표에 구분해 적는다.
- xAI 편집: JSON `/v1/images/edits`(검토 W2).
- 투명: `background:"transparent"` + `output_format:"png"|"webp"`. JPEG는 거부.
- 마스크: 원본과 같은 크기의 PNG, 알파 0인 곳이 편집 영역. 여러 참조가 있으면 첫 장에 적용.
- `input_fidelity`: gpt-image-2 계열에는 보내지 않는다(공식 문서).
- CLIProxyAPI가 `mask`/`background`를 그대로 전달하는지는 [구현 중 확인]. 전달하지 않으면 해당 기능은 대체 경로(§7.2, §7.3)로 처리하고 화면에 "대체 방식" 표시.

## 7. 핵심 기능 동작

### 7.1 생성 / 참조 편집

결과는 요청 크기로 정확히 저장하되 **맞춤 방식**을 나눈다(검토 P1-02: 1374×1145 투명 컵을 중앙 자르기로 1024²로 만들자 손잡이가 잘림):

| 용도 | 기본 맞춤 |
|---|---|
| 투명 결과(모델 투명·배경 제거) | `contain`: 비율 유지 + 투명 여백 |
| 일반 장면 | `cover`: 중앙 자르기(기존 동작). 사용자가 `contain`(여백 색 지정) 선택 가능 |
| 마스크 정확 보존 편집 | 원본 좌표계 유지(§7.2). 결과를 원본 크기로 맞춘 뒤 합성 |

기록 JSON에 요청 크기, 공급자 원본 크기, 저장 크기, 적용한 맞춤 방식을 남긴다. 투명 이미지(RGBA)는 크기 맞춤 중 알파를 보존한다.

참고: 이 프록시는 요청 크기와 관계없이 1536x1024 원본을 돌려주는 경우가 관찰됐다(`docs/SSOT.md`). 16:9 저장 시 위아래가 잘린다. 화면에 "제공자 원본 크기 → 저장 크기"를 표시한다. [작성자 판단]

### 7.2 마스크 부분 편집

1. 사용자가 캔버스에서 붓/지우개/올가미/반전/전체 지우기로 마스크를 칠한다. 붓 크기 조절. (시안 `02_proDark_maskEdit.png`)
2. 마스크를 원본 크기 PNG로 만든다(칠한 곳 알파 0).
3. 모델이 마스크를 지원하면 마스크와 함께 편집 요청. 지원하지 않으면 원본만 보내고 프롬프트로 지시.
4. **"마스크 밖은 그대로 유지"(기본 켜짐)**: 결과를 원본 크기로 맞춘 뒤, 원본 위에 **마스크 영역만** 합성한다. 정확한 규칙(검토 P1-03):
   - 기준 원본은 EXIF 방향을 반영해 디코딩한 RGBA. 마스크는 이 크기·좌표계에 고정한다.
   - 경계 부드럽게(기본 4px)는 **편집 영역 안쪽으로만** 퍼진다(가중치 = 편집 영역 ∩ 흐림). 편집 영역 밖 가중치는 정확히 0이고, 그 픽셀은 원본 RGBA를 그대로 복사한다. (전체 마스크를 단순히 흐리면 64×64 실험에서 보호 픽셀 1,560개가 바뀜)
   - 정확 보존 모드는 PNG(또는 무손실 WebP)만 허용, JPEG 금지. 최종 이미지를 다시 손실 압축하거나 다른 크기로 바꾸지 않는다.
   - **저장 후 다시 디코딩해 보호 영역 RGBA 차이가 0인지 검사**하고, 아니면 실패로 기록한다.
5. 원본·수정본 비교 슬라이더를 보여 준다. 기록 JSON에 마스크 파일 경로와 합성 여부를 남긴다(마스크 PNG는 metadata 폴더에 저장).

"자동 피사체 선택" 버튼은 §7.3의 배경 제거 모델로 얻은 피사체 영역을 마스크로 쓰는 방식. [작성자 판단]

### 7.3 투명 배경

화면의 선택지 3개(시안 `03_proDark_transparent.png`):

1. **모델 자체 투명**: 지원 모델일 때만 활성. `background:"transparent"`.
2. **생성 후 배경 제거**: 모든 모델에서 사용. 로컬에서 실행하는 배경 제거 모델(ONNX, `ort` crate). 모델 파일은 설치 파일에 넣지 않고 **처음 사용할 때 내려받는다**(진행률 표시, 해시 검증). 모델은 **재배포·상업 이용이 허용되는 라이선스**여야 한다(예: Apache-2.0/MIT 계열). 비상업 라이선스 모델(예: RMBG-1.4)은 쓰지 않는다. 구체 모델 선택은 구현자 몫이나, 구현 계획에 **모델 이름·버전·내려받기 URL·SHA-256·라이선스, ONNX Runtime 배포 방식**을 고정한다. 내려받기 실패·해시 불일치·캐시 재사용·모델 교체 정책을 포함하고, 모델이 없을 때 다른 방식으로 조용히 바꾸지 않는다. CPU 전용 PC에서 동작해야 한다.
3. **단색 배경**: 색상 지정.

공통: 저장 후 **알파 검사**: 완전 투명(α=0)·부분 투명(0<α<255)·불투명 비율을 따로 계산하고 네 모서리 알파를 확인한다. 화면에는 "완전 투명 44.8%"처럼 완전 투명 비율을 쓴다(실측: 컵 몸통이 α=253이라 `α<255` 비율은 99.9%로 무의미). 피사체 가장자리 품질은 사람이 확인하는 항목으로 둔다. 모델 자체 투명을 요청했는데 투명 픽셀이 없으면 "모델이 투명을 적용하지 않음"을 표시하고 배경 제거를 제안한다. 내보내기는 PNG/WebP만 허용, JPEG는 비활성("투명 없음").

### 7.4 병렬 생성 · 진행 표시

기존 기본값 유지(동시 30, 최대 32, 재시도 2). 카드별 상태: 대기/진행/재시도(원인 코드)/완료/실패. 상단에 완료·진행·재시도·실패 수. 취소 버튼(진행 중 요청 중단, 완료분 보존). 같은 topic으로 이어서 생성(resume). (시안 `04_lightMinimal_batchGallery.png`의 구성을 다크로)

### 7.5 갤러리 · 히스토리

- 갤러리는 **등록된 폴더 목록**의 `ImageGallery/metadata`를 읽어 목록을 만든다. 기본 등록 폴더는 앱 작업 폴더(`%USERPROFILE%\Pictures\Image Studio`, 변경 가능) 하나이고, 사용자가 프로젝트 폴더(에이전트가 `topic_root`로 쓰는 곳)를 추가·제거할 수 있다. 폴더별 필터 제공. [작성자 판단]
- 앱 전용 DB는 두지 않는다(목록은 매번 metadata JSON에서 만들고, 빠르게 하기 위한 캐시는 지워도 되는 것만 허용). CLI가 앱에 알리는 별도 색인은 만들지 않는다 → 등록하지 않은 폴더에 에이전트가 만든 이미지는 보이지 않는다(의도된 동작).
- 완료 결과와 실패 기록을 구분하고(§5.4 `status`), 기록은 있는데 이미지가 없으면 "파일 없음"으로 표시한다. 등록 폴더는 경로를 정규화해 중복을 막는다.
- 기록에서 "이 설정 다시 쓰기", "참조로 사용", 여러 장 비교, ZIP 내보내기.

## 8. 화면 설계

디자인은 **다크 계열** [사용자 결정]. 구체 방향은 **다크 작업 도구형(시안 01~03)**: 왼쪽 설정 패널, 가운데 캔버스/결과, 오른쪽 히스토리, 보라 강조색. [작성자 판단: 사용자는 다크를 선택했고 01~03과 E 중 확정 전. E의 "큰 캔버스" 장점은 결과 확대 보기 모드로 흡수]

시안 이미지(참고용, 실제 동작 아님, Git 제외 폴더):
- `E:\ai-image-generator\ImageGallery\output\studio-redesign-concepts\01_proDark_generate.png`
- `...\02_proDark_maskEdit.png`, `...\03_proDark_transparent.png`
- 비교용: `...\studio-redesign-concepts-v2\E_canvasFirst_1_generate.png`

화면 목록:

1. **생성**: 프롬프트, Avoid(접기), 모델(연결 확인된 목록만), 크기, 품질, 장수, 참조 이미지(끌어놓기, 최대 4장·장당 10MB·한 변 8192px), 배경 옵션, 생성 버튼. 결과 그리드.
2. **편집(마스크)**: §7.2.
3. **배경**: §7.3.
4. **배치 진행**: §7.4.
5. **갤러리/히스토리**: §7.5.
6. **연결 설정**: 프로필 목록·추가·수정·삭제, 연결 확인 결과(§6.2), 기본 프로필 지정. 첫 실행 시 이 화면부터.

화면 글자는 한국어 기본 [작성자 판단]. 영어 전환은 비목표.

## 9. 배포

- Tauri 번들러로 Windows 설치 파일(NSIS `.exe`) 생성. WebView2가 없으면 설치 중 내려받기.
- 설치 파일 합격 기준은 개발 PC 라이브러리에 기대지 않고 **Python 없는 노트북에서** 설치·실행·생성·편집·투명 저장·배경 제거가 되는 것. WebView2, ONNX Runtime DLL, VC 런타임을 설치 경로에서 해결한다.
- "Python 없는 앱"은 설치한 사용자 앱의 실행 요구 사항을 뜻한다. 저장소의 데이터셋용 Python 스크립트 유지와 모순되지 않는다.
- 서명 없음(1차). 설치 안내에 SmartScreen "추가 정보 → 실행" 설명.
- 설치 파일에 포함 금지: API 키, 사용자 프로필, 결과 이미지, 배경 제거 모델 파일.
- CLI(`image-cli.exe`)도 설치 폴더에 함께 넣고, 원하면 PATH 등록. 에이전트 스킬은 이 CLI를 사용하게 된다(단계 6).

## 10. 단계 (각 단계 끝에 검증)

| 단계 | 내용 | 완료 기준 |
|---|---|---|
| 0. 연결 사전 점검 | 노트북에서 **기존 Python CLI**로 `http://<프록시 PC의 Tailscale 주소>:8317/v1` `--list-models` 및 1장 생성. 프록시가 `0.0.0.0:8317`에 열려 있는지, Windows 방화벽이 Tailscale 쪽 8317 인바운드를 허용하는지 확인 | 노트북에서 기존 CLI로 모델 조회·1장 생성·저장. **2026-10-18 현재 미통과**: 데스크톱 `CLIPROXY_BASE_URL` 미설정, Tailscale `NoState`, `<이전 주소>:8317` TCP 시간 초과, 노트북 오프라인. 방화벽 허용 규칙은 존재. 현재 Tailscale 주소를 다시 확인하고 과거 IP를 그대로 복사하지 않는다 |
| 1. 엔진+CLI 동등성 | `image-core`, `image-cli`로 §5 이전, §5.4 규칙 고정 | Rust 통합 테스트 통과(§5.4 fixture 포함), Python과 동등성 비교 통과(의도된 차이 제외), 실제 프록시 1~2장 성공 |
| 2. 프로필·공식 API | §6 전체, `keyring`, 오류 구분 | 프로필별 연결 확인 테스트, 가능한 공식 API는 실제 1장 [사용자 키 있을 때] |
| 3. 앱 1차 | Tauri 앱: 연결 설정, 생성, 배치 진행, 갤러리 | 이 PC에서 앱으로 생성·취소·이어하기 확인 |
| 4. 마스크·투명 | §7.2, §7.3, 알파 검사, 프록시 `mask`/`background` 전달 여부 확인 | 마스크 밖 픽셀 동일성 테스트, 알파 검사 테스트, 실제 1~2장 |
| 5. 설치 파일·첫 실사용 | NSIS 설치 파일, 노트북 설치 → Tailscale → 이 PC 프록시 | G5 달성 |
| 6. Python 정리 | 스킬·문서를 Rust CLI로 전환, MCP를 Rust(`rmcp`)로 이전, Python 코드 제거 | 스킬로 생성 성공, MCP 도구 동작, Python 없이 전체 테스트 통과 |

단계 0은 코드 작업 없이 바로 할 수 있고, 결과에 따라 §6.2 오류 문구를 다듬는다.

**단계 0과 함께 하는 Python 보안 수정**: 이전 기간에도 에이전트가 기존 Python CLI를 쓰므로, 검토 P1-01(생성 경로가 리다이렉트를 따라가며 `Authorization`을 넘김)은 `scripts/gen_image.py`에서 바로 고친다.

### 단계 6 세부 [작성자 판단]

- **에이전트 PC의 CLI 확보**: 이 저장소 checkout에서 `cargo build --release -p image-cli`로 빌드하고, 스킬은 checkout 안의 `target/release/image-cli(.exe)`를 찾는다(지금 `.venv`를 찾는 방식과 같은 위치 규칙). `scripts/setup.py`·`sync_skills.py`의 역할(빌드 확인, 스킬 링크)은 `image-cli setup`/`image-cli setup --check` 하위 명령으로 옮긴다. GitHub Release로 미리 빌드한 실행 파일을 배포하는 것은 이후 과제(외부 공개 작업이라 별도 승인).
- **MCP 이전 범위**: 기존 20개 도구의 입력·출력·오류·취소·재개 계약을 이관 표로 관리하며 전체를 `rmcp`로 옮긴다(계획·실행·조회·취소·재개, 프리셋, 이미지 검색·메타데이터·미리보기, 크롭·리사이즈·컨택트시트, `metadata/_mcp` 상태 형식). 이전 전까지 Python MCP는 그대로 둔다.
- **데이터셋 스크립트**(`t2in_event_hazards.py`, `gen_art_candidates.py`): `gen_image.py` 직접 호출을 Rust CLI(jobs 모드) 호출로 바꾼 뒤 Python 표준 라이브러리만 쓰는 독립 스크립트로 남긴다. 엔진 Python 코드(`gen_image.py`, `ai_image.py`, `gen_batch.py`, `gallery.py`, `image_core.py`, MCP)는 제거한다.

## 11. 구현자에게 맡기는 것

- 화면 프레임워크 세부(Svelte 기본, 근거 있으면 React), 컴포넌트 구조, 상태 관리.
- crate 내부 모듈 구성, 오류 타입 설계.
- 배경 제거 모델의 구체 선택(라이선스 조건 준수 시).
- 마스크 경계 부드럽게 처리 방식, 캔버스 구현(HTML Canvas 등).
- 진행 이벤트를 화면에 보내는 방식(Tauri 이벤트/채널).
- 테스트 fixture 구현 방식.

## 12. 참고

- 투명/마스크 조사(2026-10-18 대화): OpenAI Images API 문서(create/edit), OpenAI Cookbook 투명 에셋, 커뮤니티의 gpt-image-2 투명 중단 보고, Gemini 투명 미지원 보고.
- 외부 검토: `docs/superpowers/reviews/2026-10-18-image-studio-rust-app-review.md`(실측 7건, P1-01~05, 증거 `ImageGallery/metadata/studio-api-review-20261018/`).
- 기존 동작·실측: `docs/SSOT.md`(모델 상태, 병렬 실측, 머신 토폴로지), `tools/ai_image.py --schema`.
