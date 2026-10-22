# Image Studio Rust 앱: 설계·코드·API 실동작 검토

- 검토일: 2026-10-18
- 대상 장치: jisung-desktop / DESKTOP-SB818KQ
- 워크스페이스: `E:\ai-image-generator`
- 설계 문서: `E:\ai-image-generator\docs\superpowers\specs\2026-10-18-image-studio-rust-app-design.md`
- 판정: **Rust 공통 엔진 + Tauri/Svelte 방향은 적합하다. 아래 P1 항목을 설계와 구현 계획에 반영해야 한다. 현재 모든 연결 프로필과 완성 앱의 검증이 끝난 상태는 아니다.**
- 작업 범위: 설계와 기존 Python 구현 검토, 공식 API 문서 대조, 기존 테스트, 실제 프록시 이미지 요청, 로컬 진단 실험. 운영 코드·원본 설계·연결 설정은 수정하지 않았고 커밋하지 않았다.

## 1. 확인된 결과

CLIProxyAPI를 통한 GPT/Gemini의 생성과 참조 이미지 수정은 실제로 성공했다. Sunburst 요청에서는 실제 알파 채널을 가진 PNG를 받았다. 마스크 밖 원본 픽셀 보존은 로컬 합성 진단본에서 달성했다. 반면 Grok 요청은 공급자 지출 한도로 실패했고, 노트북/Tailscale 경로는 통과하지 못했다.

프록시 성공은 OpenAI·Google·xAI 공식 API 직접 연결 성공과 다르다. 확인한 프로세스·Windows 사용자 환경변수 및 앱 프로필에는 공식 API 직접 연결용 키가 없었다. 이 검토는 새로운 키를 만들거나 계정·결제를 설정하지 않았다.

| 점검 | 결과 | 근거 |
|---|---|---|
| 체크아웃 환경 | 정상 | 기존 가상환경에서 scripts/setup.py --check 통과 |
| 기존 자동 테스트 | 34개 통과, 12.799초 | python -m unittest discover -s tests -v |
| MCP health | 정상 | 실제 워크스페이스·프록시 주소·키 설정 여부 확인 |
| MCP plan_generation | 정상 | 계획만 생성했고 이 MCP 계획의 생성 실행은 하지 않음 |
| Rust/Tauri 앱 | 아직 구현 전 | Cargo.toml 및 신규 앱 구현이 없는 현재 저장소 확인 |
| CLIProxyAPI | 로컬 동작 확인 | 8.0.32, commit 911f49cb, build 2026-10-16T18:58:26Z |
| 기본 연결 주소 | 기존 자동 규칙에 의존 | 실제 해석 주소 http://127.0.0.1:8317/v1, 명시적 CLIPROXY_BASE_URL 없음 |
| 노트북 연결 | 미검증/차단 | jisung-labtop 오프라인, 아래 네트워크 점검 참조 |

현재 테스트 통과는 기존 Python 동작의 기준선이다. 아직 없는 Rust 프로필·마스크 UI·배경 제거·설치 파일까지 검증한 것으로 해석하면 안 된다.

## 2. 실제 이미지 요청 7건

공통 진단 소재는 파란 도자기 컵과 금색 손잡이였다. 참조 수정은 컵 몸통을 녹색으로 바꾸는 요청이었다. 모든 요청은 사용자의 기존 CLIProxyAPI를 경유했다. Grok 실패 뒤 자동 재시도나 다른 모델로의 대체를 하지 않았다.

| 요청 | 요청 모델 | 결과 | 소요 시간 |
|---|---|---|---:|
| 텍스트로 이미지 생성 | gpt-image-2.5 | 성공, 저장본 1024×1024 | 20.76초 |
| 텍스트로 이미지 생성 | gemini-3.1-flash-image | 성공, 저장본 1024×1024 | 15.73초 |
| 텍스트로 이미지 생성 | grok-imagine-image-2.0 | 실패: personal-team-blocked:spending-limit | 1.21초 |
| 참조 이미지 수정 | gpt-image-2.5 | 성공, 녹색 컵 확인 | 31.40초 |
| 참조 이미지 수정 | gemini-3.1-flash-image | 성공, 녹색 컵 확인 | 12.70초 |
| 투명 PNG 진단 요청 | gpt-image-2.5-sunburst | HTTP 200, 실제 RGBA | 57.05초 |
| 마스크 편집 진단 요청 | gpt-image-2.5 | HTTP 200, 이후 로컬 합성 검증 | 26.19초 |

위 시간은 각 1건의 관측값이며 성능 비교나 SLA가 아니다. 모델 목록에는 처음 9개 이미지 모델 ID가 보였지만, 9개 모델 모두의 생성을 검증한 것은 아니다. 목록과 할당량은 별개이며 이후 목록도 달라질 수 있다.

Gemini 기본 생성 응답은 modelVersion으로 gemini-3.1-flash-image를 반환했다. GPT 계열 진단 응답에는 내부에서 최종 선택된 모델 ID가 없었다. 요청한 gpt-image-2.5가 내부적으로 어느 변형으로 처리됐는지는 추정하지 않는다.

### 2.1 투명 PNG 실측

- 요청 모델은 gpt-image-2.5-sunburst, 요청은 transparent + PNG였다.
- 공급자 원본은 요청한 1024 정사각형과 다른 **1374×1145 RGBA**였다.
- 알파 최솟값 0, 최댓값 255, 네 모서리 모두 0이었다.
- 전체 1,573,230픽셀 중 **704,173픽셀, 44.7597%가 완전 투명(alpha=0)**이었다.
- 컵 내부에도 alpha=253 등 부분 투명 값이 많았다. alpha<255 비율 99.916%를 그대로 “배경 제거율”이라고 표시하면 잘못된 설명이 된다.
- 원본, 기존 정규화 결과, 비율 유지 진단본을 직접 열어 확인했다. 이미지 미리보기의 회색 바탕과 실제 파일의 알파 검사는 구분했다.

실제 투명 정보가 있다는 사실은 확인했다. 피사체 가장자리와 몸통의 부분 투명도 품질까지 모든 소재에서 합격했다는 뜻은 아니다.

### 2.2 마스크 실측

마스크 alpha=0을 편집 영역으로 사용했다. 공급자 결과는 1254×1254였고 원본의 1024×1024 좌표계로 정규화한 뒤 비교했다.

| 비교 | 마스크 밖 변경 픽셀 |
|---|---:|
| 공급자 결과를 정규화한 이미지 대 원본 | 912,769 / 919,876, 약 99.2274% |
| 편집 영역에만 합성한 진단본 대 원본 | **0** |

첫 번째 차이에는 공급자 재생성과 크기 정규화의 영향이 함께 들어 있다. 이것만으로 공급자가 mask 필드를 무시했다고 단정하지 않는다. 다만 공급자 결과를 그대로 저장해서는 “마스크 밖 원본과 픽셀 단위 동일”을 보장할 수 없다는 점은 확인했다.

4픽셀 feather를 단순히 전체 마스크에 적용하는 별도 64×64 로컬 실험에서는 보호 영역 1,560픽셀이 바뀌었다. feather 범위를 원래 편집 영역 안으로 제한하면 변경은 0이었다. 실제 컵에 빨간 별을 추가한 합성 진단본도 직접 확인했다.

투명·마스크 실험은 현 CLI에 없는 기능을 검증하기 위한 별도 진단 코드다. 앱이나 정식 CLI에 해당 기능이 이미 구현됐다고 보고하지 않는다.

## 3. 우선 수정 사항

P1은 키·원본 보존·기능 적용·작업 결과 신뢰성에 영향을 주므로 구현 계획 확정에 반영해야 할 항목이다. 아래에서 “재현”과 “코드상 위험”을 구분한다.

### P1-01. 실제 생성 요청의 리다이렉트가 인증 헤더를 전달한다 — 재현

근거: `E:\ai-image-generator\scripts\gen_image.py`의 _call_json_blocking, 348행 이후, urllib.request.urlopen 호출. 모델 목록 연결 검사는 `E:\ai-image-generator\tools\image_core.py`의 check_connection에서 NoRedirect를 쓰지만 생성 경로는 다르다.

로컬 HTTP 서버 두 개와 가짜 토큰으로 실제 생성 HTTP 헬퍼를 호출했다. 127.0.0.1에서 localhost의 다른 포트로 302 이동했을 때 목적지에 Authorization이 전달됐다. 모델 목록 검사에서는 같은 이동이 차단됐다. 외부 API 호출 0건이며 실제 사용자 자격 증명은 이 실험에 사용하지 않았다. 실제 키 유출 사고가 있었다는 주장은 아니다.

수정 요구: 모델 목록·생성·편집에 같은 URL 검사와 리다이렉트 정책을 적용한다. Rust HTTP 클라이언트는 리다이렉트를 기본 차단하고 3xx를 명시적으로 표시한다. URL이 다른 프로필로 바뀌어도 기존 키를 붙여 재요청하지 않는다. API 응답 오류를 표시할 때 키와 인증 헤더를 제거한다.

완료 기준: 서로 다른 호스트/포트로 이동하는 로컬 fixture에서 목적지에 인증 요청이 도착하지 않고, 생성·편집·모델 조회 모두 같은 결과를 낸다.

### P1-02. 모든 결과를 중앙 자르기하면 투명 소재가 잘린다 — 재현

근거: `E:\ai-image-generator\scripts\gen_image.py`의 normalize_image_bytes, 406행 이후. 현재 ImageOps.fit의 중앙 자르기 + LANCZOS가 설계 §7.1에도 일괄 적용된다.

RGBA와 알파 자체는 보존됐다. 그러나 실제 투명 컵 원본을 1024×1024로 바꿀 때 피사체 오른쪽 약 42.5 원본 픽셀이 잘려 손잡이가 손실됐다. 같은 결과를 contain + 투명 여백으로 처리한 진단본에서는 피사체 전체가 보존됐다.

수정 요구: 투명 소재 기본값은 비율 유지와 투명 여백으로 정한다. 장면 이미지의 cover/중앙 자르기는 명시적 선택으로 둔다. 요청 크기, 공급자 원본 크기, 최종 저장 크기와 적용한 변환을 메타데이터에 남긴다.

완료 기준: 비정사각 투명 fixture를 정사각 출력으로 저장해도 피사체가 잘리지 않고 알파가 남는다. 마스크 편집에서는 아래 원본 좌표계 규칙을 우선한다.

### P1-03. 마스크 보존 보장의 좌표·경계·저장 조건을 고정해야 한다 — 설계 보완

설계 §7.2의 로컬 합성 방향은 맞다. 다만 “4px feather”만으로는 마스크 밖 불변 조건이 성립하지 않는다.

수정 요구: EXIF 방향을 반영한 기준 원본의 크기와 좌표계를 고정하고 마스크를 그 좌표에 연결한다. 합성 가중치는 원래 편집 영역 밖에서 정확히 0이어야 한다. 보호 영역은 기준 원본의 RGBA 값을 직접 복사한다. 정확 보존 모드에서는 최종 전체 이미지를 다시 손실 압축하거나 임의 크기로 변환하지 않는다.

완료 기준: PNG를 기본 검증 형식으로 사용하고, 디스크 저장 후 다시 디코딩해 보호 영역의 RGBA 차이가 0인지 확인한다. WebP를 허용하려면 선택한 무손실 인코딩 옵션에서도 이 조건을 통과해야 한다. JPEG는 정확 보존 모드에서 사용할 수 없다.

### P1-04. 새 기능 필드가 조용히 무시되지 않도록 계약을 정한다 — 현재 CLI에서 재현

근거: `E:\ai-image-generator\tools\ai_image.py`의 run_single/run_jobs와 자식 프로세스 인자 구성. _run_with_retries를 mock한 오프라인 검사에서 mask, background, remove_background, keep_outside_mask, profile이 요청에 있어도 실제 실행 인자로 전달되지 않았다. 존재하지 않는 mask 경로도 이 새 필드 검증을 거치지 않았다.

현재 CLI에 아직 없는 기능이라는 점 자체는 예상 범위다. 문제는 이 CLI를 새 앱의 기능 검증에 그대로 사용하면 “성공”을 잘못 판단할 수 있다는 것이다. 해당 진단의 mock 성공은 이미지 생성 성공이 아니다.

수정 요구: Rust 입력 스키마에 필드별 적용 위치와 지원 조건을 정의한다. 미지원 기능 조합은 요청 전 구조화된 오류로 반환한다. 결과에는 실제 적용한 배경 처리 방식·마스크 합성 여부·선택 프로필을 기록한다. CLI와 MCP의 입력 스키마도 함께 갱신한다.

완료 기준: 존재하지 않는 마스크, native alpha 미지원 모델, 서로 충돌하는 배경 처리 옵션이 유료 호출 전에 실패한다. 정상 옵션은 생성 엔진까지 전달됐다는 fixture 검증이 있다.

### P1-05. 파일 쌍의 일관성과 resume의 유효성 검사가 부족하다 — 코드상 위험

근거: `E:\ai-image-generator\scripts\gen_image.py` 625행 이후의 요청 전 메타데이터 기록과 나중의 이미지 직접 쓰기, `E:\ai-image-generator\tools\ai_image.py` 366/511행 부근의 is_file 기반 resume.

`E:\ai-image-generator\scripts\gallery.py`의 write_json은 임시 파일과 os.replace로 메타데이터 한 파일을 원자적으로 쓴다. 이 점은 올바르다. 그러나 이미지와 메타데이터 두 파일이 하나의 완료 결과가 되는 시점까지 보호하지는 않는다.

동일 출력 경로를 두 프로세스가 동시에 선택하면 둘 다 API 비용을 쓰고, 한 프로세스의 이미지에 다른 프로세스의 메타데이터가 짝지어질 수 있다. 이미지 쓰기 도중 중단된 파일도 is_file만으로 resume가 건너뛸 수 있다. 이 동시성/강제 중단 시나리오는 이번에 실제 재현하지 않았으며 코드 흐름에서 도출한 위험이다. 실제 Grok 실패에는 이미지 없는 메타데이터가 남았다.

수정 요구: 요청 전 출력 경로를 예약하고, 임시 이미지의 디코딩·크기·해시를 검증한 뒤 완료 상태를 공개한다. 상태 파일만으로 완성품이 되지 않게 한다. resume는 파일 존재 외에 완료 상태와 요청 식별자, 이미지 유효성을 확인한다. 갤러리는 실패 기록과 완성 이미지를 구분한다.

완료 기준: 동일 경로 동시 시작, 저장 중 강제 종료, 손상 이미지와 다른 프롬프트의 resume fixture가 결과를 잘못 재사용하지 않는다. 별도 데이터베이스 없이 파일 메타데이터로 구현할 수 있다.

## 4. API 프로필 설계 수정

연결 프로필은 주소와 키만 바꾸는 설정이 아니라 인증·모델 목록·요청 형식·응답 해석·기능 지원의 조합이어야 한다. “OpenAI 호환”이라는 이름만으로 이미지 편집 규격까지 같다고 가정하지 않는다.

| 프로필 | 이번 확인 | 설계 반영 |
|---|---|---|
| CLIProxyAPI | 로컬 경유 GPT/Gemini 생성·참조 수정 성공, Sunburst 투명 진단 성공, Grok 지출 한도 실패 | 프록시 별칭·버전·기능 검증 날짜 기록. 요청 ID와 응답 모델 ID 분리 |
| OpenAI 직접 | 공식 문서 대조, 직접 키로 실호출하지 않음 | 현재 편집 문서에는 JSON images/mask 형식과 multipart 예제가 모두 있음. “JSON 미검증/미문서화” 구분을 고침 [W1] |
| Gemini 직접 | 프록시 Gemini만 실호출, 직접 연결은 미검증 | native /v1beta/models 및 페이지 처리, x-goog-api-key 인증을 별도 어댑터로 구현 [W3][W4] |
| xAI 직접 | 공식 문서 대조, 직접 키로 실호출하지 않음 | 참조 이미지 편집을 지원함. JSON /v1/images/edits 형식을 구현하고 refs 불가 표기를 고침 [W2] |
| 기타 호환 | 기존 프록시 설정 구조만 확인 | 공급자마다 generation/edit/URL 응답/alpha 지원을 따로 검증한 기능 표를 유지 |

OpenAI 공식 문서는 sunburst/flare의 투명 배경과 PNG/WebP 출력 조건을 명시한다 [W1]. 이번 bare gpt-image-2.5 성공은 현재 프록시의 해당 ID에 대한 증거다. 다른 공식 프로필에서도 같은 별칭이 통한다고 추정하거나 내부 변형을 임의 지정하지 않는다.

현재 Python의 Grok 참조 차단은 클라이언트 구현 제한이다. 이를 xAI 서비스 전체의 기능 제한으로 문서화하면 구현 범위를 잘못 줄이게 된다 [W2].

추가 계약:

- 프로필 선택은 provider + base URL + credential reference를 하나로 확정한다. 명시한 프로필이 없거나 불완전하면 다른 공급자의 키를 빌려 쓰지 않고 오류를 낸다.
- 기존 환경변수 읽기와 새로 입력한 키의 Credential Manager 저장을 구분한다. “평문 키를 저장하지 않음”에 기존 환경변수 호환의 범위도 설명한다.
- URL의 /v1 자동 보정은 OpenAI형 어댑터에 한정한다. Gemini native 주소와 기타 공급자의 경로를 공통 규칙으로 바꾸지 않는다.
- 모델 조회 성공, 인증 성공, 실제 생성 가능, 요청 기능 지원을 별도 상태로 표시한다. Grok 사례처럼 모델 목록에 있어도 할당량 때문에 생성이 불가능할 수 있다.
- 오류 구조에 category, provider_code, http_status, retryable을 남긴다. 401/403을 모두 키 오타로 분류하지 말고 권한·지출 한도·정책·기능 미지원도 구분한다.
- 공급자 오류 원문을 무제한 전달하거나 stderr 마지막 줄만으로 의미를 잃지 않도록 정제된 구조를 쓴다. 근거는 ai_image.py의 _run_child 결과 요약과 gen_image.py의 HTTP 오류 출력이다.
- 공식 프로필에 기존 프록시의 동시 실행 30을 무조건 적용하지 않는다. 호환 기본값과 프로필별 동시성·할당량 정책을 분리한다. 한 건의 측정으로 최적 동시성을 주장하지 않는다.

## 5. 추가로 주신 네 가지 결정

| 결정 | 검토 의견 | 문서에 덧붙일 완료 조건 |
|---|---|---|
| 등록한 폴더만 갤러리에 표시, DB 없음 | 적합 | 완성 결과/실패 메타데이터 구분, 누락 파일 처리, 중복 루트와 경로 정규화, 캐시는 삭제해도 재생성 가능 |
| 특정 PC/Tailscale 주소 하드코딩 제거 | 반드시 유지 | 명시 프로필 → 환경변수 → 기본 프로필 우선순위 fixture, 아무 설정이 없으면 네트워크 호출 전 오류 |
| cargo build CLI, MCP 전체 Rust 이관, 데이터셋 Python 스크립트 유지 | 적합 | 현재 20개 MCP 도구의 스키마·오류·취소·재개·결과를 이관 표로 관리. 데이터셋은 Rust CLI 호출 계약 검증 |
| 사용하지 않는 프로젝트 Responses 경로 제거 | 적합 | 지원하지 않는 모델/경로에 명시적 오류. “이 프로젝트에서 미사용”이라는 제거 이유를 유지 |

“Python 없는 앱”은 설치된 사용자 앱의 실행 요구 사항으로 정의하면 데이터셋용 Python 유지와 모순되지 않는다. “저장소에서 Python 파일이 하나도 없어야 한다”는 목표로 해석하면 안 된다.

작업 도구형 어두운 화면, Svelte, 한국어, 서명 없는 1차 설치 파일은 현재 범위에서 유지할 수 있다. 시안은 확인했지만 실제 Tauri UI와 설치 파일은 아직 없다. 화면의 연결 배지는 모델 조회와 생성 가능 상태를 구분하고, 투명 배지와 출력 크기는 실제 저장 파일에 근거하도록 설계한다.

## 6. 0단계 연결 검증의 현재 상태

- 데스크톱의 loopback 프록시 호출은 성공했다.
- CLIPROXY_API_KEY는 프로세스/사용자 환경에서 설정 여부를 확인했으나 값은 표시하지 않았다.
- CLIPROXY_BASE_URL은 해당 환경에 없고 앱 profiles.json도 없었다.
- Tailscale BackendState는 NoState였고 활성 Self IP를 확인하지 못했다.
- 인증 없이 TCP만 검사한 <이전 주소>:8317 연결은 3초 뒤 timeout이었다.
- Tailscale 대역에 대한 TCP 8317 허용 방화벽 규칙은 존재했다. 따라서 방화벽 규칙 하나가 없다는 식으로 원인을 단정하지 않는다.
- jisung-labtop은 오프라인이었다. 실제 노트북에서 생성하는 0단계 합격 조건은 통과하지 못했다.

다음 합격 조건은 양쪽 장치의 Tailscale 연결 상태를 복구·확인한 뒤, 데스크톱에는 명시적인 로컬 주소, 노트북에는 현재 확인된 Tailscale 주소를 설정하고 노트북에서 기존 CLI로 1장을 생성·저장하는 것이다. 현재 주소를 확인하지 않고 문서의 과거 IP를 새 기본값으로 복사하지 않는다.

이번 검토에서 Tailscale 로그인, 서비스 재시작, 환경변수·방화벽·키 변경을 수행하지 않았다.

## 7. 로컬 배경 제거 및 배포의 남은 구현 조건

설계의 ONNX/ort 방향만으로 배경 제거 완료를 선언할 수 없다. 이번 환경에서는 rembg/onnxruntime 기반 실행이나 선택된 ONNX 모델을 검증하지 않았다. 새 Rust 실행 경로도 아직 없다.

구현 계획에는 선택한 모델 가중치의 이름·버전·다운로드 URL·해시·배포 조건과 추론 코드/런타임의 배포 조건을 각각 기록한다. 첫 다운로드 실패·해시 불일치·캐시 재사용·모델 교체 정책을 포함한다. 모델이 없을 때 native/단색으로 조용히 바꾸지 않고 실제 선택 상태를 보여 준다.

설치 파일의 완료 조건은 개발 PC의 라이브러리에 기대지 않고 노트북에서 실행되는 것이다. WebView2와 ONNX Runtime DLL, 필요한 VC 런타임 등을 설치 경로에서 해결하고, CPU만 있는 환경의 배경 제거와 RGBA 저장을 검증한다. 서명 없는 설치 파일이라는 선택과 실행 가능성 검증은 별개다.

## 8. 구현 계획에 넘길 작업 단위와 합격 기준

| 단계 | 작업 단위 | 합격 기준 |
|---|---|---|
| 0 연결 기준선 | Tailscale 상태와 두 장치 주소 명시 | 노트북에서 기존 CLI 모델 조회·1장 생성·저장 통과. 현재는 미통과 |
| 1 Rust 엔진/CLI | JSON 계약 fixture, URL/리다이렉트, 파일 예약·완료·resume, 취소·오류 | 기존 지원 시나리오의 정규화된 JSON·파일 결과 일치. P1 보안/저장 fixture 통과 |
| 2 연결 프로필 | 5종 어댑터, Credential Manager, 모델 기능 표, 할당량 오류 | 프로필별 인증·목록·생성·참조 편집의 적용 가능한 검사 통과. 미설정 프로필은 미검증으로 표시 |
| 3 앱의 기본 작업 흐름 | Tauri/Svelte에서 프로필 선택 → 프롬프트 → 생성 → 저장 → 갤러리 | CLI와 같은 엔진 결과, UI 응답성·취소·오류 표시 확인 |
| 4 마스크/투명 | 원본 좌표 합성, 경계 제한 feather, contain, native alpha, 로컬 제거 | 저장 후 보호 픽셀 차이 0, 실제 알파 확인, 피사체 무잘림, 모델 패키지 검증 |
| 5 Windows 설치 | NSIS, WebView2/추론 런타임, Pictures 기본 경로 | Python 없는 노트북에서 설치·실행·생성·편집·투명 저장·제거 흐름 확인 |
| 6 이전 종료 | 모든 MCP 도구/셋업·스킬 동기화/데이터셋 호출 이전 | Python 엔진 호출이 없어지고 모든 계약 검증 통과한 뒤 제거 |

마스크 합성·투명 크기 변환·파일 완료 처리처럼 엔진 구조를 좌우하는 규칙은 GUI를 만들기 전에 1단계 설계에 고정한다. 화면 기능의 전체 구현은 원래 3~4단계를 유지할 수 있다.

Python/Rust 비교는 동일한 로컬 HTTP fixture를 사용한다. 비결정적 실제 생성 이미지를 바이트 단위로 같게 만들려는 검증이 아니다. 시간·절대 경로·소요 시간 같은 환경값은 정규화하고, 하드코딩 제거·미사용 Responses 제거·새 필드는 의도적 차이 목록에 기록한다.

기존 ai_image.py의 결과 종료 코드는 성공 0/결과 실패 2이고, 입력 등 치명적 오류는 1 경로가 있다. RDC/PowerShell 래퍼의 exit_code=1을 Python 프로세스의 실제 반환값과 혼동하여 계약 결함으로 기록하지 않는다.

이번 결과로 사용자에게 다시 선택을 요청해야 할 설계 질문은 없다. Grok 할당량과 노트북 온라인 상태, 직접 API 프로필의 자격 증명은 추후 해당 연결의 실검증을 위한 환경 조건이다.

## 9. 증거와 산출물

집계 증거: `E:\ai-image-generator\ImageGallery\metadata\studio-api-review-20261018\review-evidence.json`

| 자료 | 절대 경로 |
|---|---|
| 기본 GPT 생성 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-20261018\01_gpt_generate.png` |
| 기본 Gemini 생성 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-20261018\02_gemini_generate.png` |
| GPT 참조 수정 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-edit-20261018\01_gpt_edit.png` |
| Gemini 참조 수정 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-edit-20261018\02_gemini_edit.png` |
| 투명 공급자 원본 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-transparent-20261018\01_provider.png` |
| 기존 중앙 자르기 결과 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-transparent-20261018\02_existing_normalizer.png` |
| 비율 유지 투명 여백 진단본 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-transparent-20261018\03_contain_prototype.png` |
| 마스크 공급자 원본 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-mask-20261018\01_provider.png` |
| 보호 영역 보존 합성 진단본 | `E:\ai-image-generator\ImageGallery\output\studio-api-review-mask-20261018\02_local_composite.png` |
| 새 필드/feather 오프라인 증거 | `E:\ai-image-generator\ImageGallery\metadata\studio-api-review-20261018\offline-contract-result.json` |
| 가짜 토큰 로컬 리다이렉트 증거 | `E:\ai-image-generator\ImageGallery\metadata\studio-api-review-20261018\redirect-security-result.json` |

진단 스크립트와 각 요청의 메타데이터는 위 ImageGallery/metadata 아래에 함께 보존했다. 이미지 진단 스크립트는 다시 실행하면 유료 API를 호출할 수 있으므로, 검토 재현 시 이미 저장된 결과와 오프라인 검사를 먼저 사용한다. 이 보고서는 원본 설계를 대체하지 않는 별도 검토 기록이다.

## 10. 외부 근거

공식 문서 확인일: 2026-10-18. 아래 문서는 공급자 규격의 근거이고, 로컬 환경에 대한 결론은 위 코드·테스트·저장 이미지가 근거다.

- [W1] OpenAI Images edit: https://developers.openai.com/api/reference/resources/images/methods/edit
- [W2] xAI Image Editing: https://docs.x.ai/developers/model-capabilities/images/editing
- [W3] Gemini Models API: https://ai.google.dev/api/models
- [W4] Gemini API key authentication: https://ai.google.dev/gemini-api/docs/api-key

