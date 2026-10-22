# Image Studio 단계 2·3 추가 변경 리뷰

- 검토일: 2026-10-19 KST / 2026-10-18 UTC
- PC·저장소: jisung-desktop, E:\ai-image-generator
- 브랜치: feat/rust-engine-cli
- 검토 HEAD: 6cb4f626f3c5f4819d747924df20224eb01c40cd
- 비교 기준: 이전 검토 HEAD 27e6a02 이후 35커밋, 변경 파일 149개
- 판정: **수정 후 재검토.** 기존 10개 지적은 수정됐지만, 새 연결·앱 경로에서 아래 문제가 확인됐다.
- 범위: 연결 프로필 5종, 키 저장과 연결 해석, 공급자 요청·응답, 생성 서비스와 IPC, Svelte 생성·연결·갤러리, 취소·기록 재개, 참조와 투명 표시, 테스트 및 기존 실물 결과.

## 1. 핵심 판단

Rust 212개, Python 39개, 프런트엔드 단위테스트 132개가 통과했다. Clippy, Svelte/TypeScript 검사, 프런트엔드 빌드도 통과했다. 이 수치는 재실행으로 확인했다.

그러나 **프로필 변경 중 이전 주소와 새 키가 결합되는 P1 문제**가 결정적인 교차 실행 순서에서 재현됐다. Gemini HTTP 200 오류의 키 정제 누락, 잘못된 연결 선택, 작업 복원 손실, 입력 준비 전 생성, 모델별 해상도 계약 및 투명 표시 문제도 확인했다. 따라서 테스트 통과만으로 단계 2·3의 기능·보안 합격을 선언하기는 어렵다.

설계 문서가 단계 3을 “부분 통과”로 적고 실제 앱 창의 생성·취소·이어하기·닫기 검증을 남긴 것은 현재 증거와 맞다. 단계 4의 마스크 편집·로컬 배경 제거, 단계 5 설치 파일, 단계 6 Python/MCP 이전 완료를 이번 변경의 성과로 계산하지 않았다.

| ID | 우선순위 | 문제 | 확인 방법 |
|---|---|---|---|
| R1 | P1 | 프로필 수정 중 이전 주소 + 새 API 키 결합 | 실제 save_profile·resolve, 임시 SecretStore로 결정적 재현 |
| R2 | P2 | Gemini HTTP 200 차단 오류의 키 문자열이 기록·이벤트에 남음 | 가짜 HTTP 공급자를 통한 실제 서비스 실행 |
| R3 | P2 | 이름이 env인 프로필과 환경변수 연결 충돌 | 실제 폼 변환 함수 실행 + Rust 이름 검증 코드 |
| R4 | P2 | 기록 재개에서 원래 요청 장수와 연결을 복원하지 못함 | 실제 모의 UI + Rust 취소 서비스 + 소스 대조 |
| R5 | P2 | 참조 검사 완료 전 참조 없이 생성 시작 가능 | 실제 생성 컨트롤러, 지연된 검사와 가짜 IPC |
| R6 | P2 | 공식 모델별 해상도 제한을 적용하지 않음 | 실제 요청 구성 함수 + 공식 문서 |
| R7 | P2 | 손상된 참조 PNG가 검사와 API 호출까지 통과 | 170바이트 PNG를 41바이트로 자른 실제 서비스 재현 |
| R8 | P2 | 완전히 불투명한 이미지가 뷰어에서 “투명”으로 표시됨 | 실제 Svelte Viewer SSR 렌더 + 실물 메타데이터 |
| R9 | P2 | 인증 미확인인 HTTP 오류를 인증 실패 단계로 표시 | 실제 상태 변환 함수, 404·503·302 입력 |
| R10 | P2 | 주소·키 변경 후에도 이전 서버의 생성 성공 표시 유지 | 실제 ProfileStore 갱신·상태 조회 |

P1은 배포 전에 우선 수정해야 할 안전성 문제다. P2는 해당 기능을 완료로 판정하기 전에 고쳐야 할 동작·계약 문제다. R4의 장수 손실은 첨부 보고서에 공개된 미완료 항목을 재확인한 것이며, 새 발견으로 포장하지 않았다.

## 2. 발견 사항과 수정 방향

### R1. 프로필 주소와 API 키가 서로 다른 버전으로 읽힌다 — P1

기존 프로필의 주소 A·키 A를 주소 B·키 B로 바꿀 때 앱과 CLI는 키를 먼저 바꾸고 프로필 JSON을 나중에 바꾼다. resolve는 프로필 파일과 키를 별도로 읽고 쓰기 잠금에 참여하지 않는다.

임시 프로필과 MemoryStore를 이용해 새 키 저장 직후 가능한 읽기를 실행했다. 실제 save_profile·resolve가 다음 조합을 반환했다.

~~~json
{
  "during_update": {
    "resolved_base_url": "http://127.0.0.1:18081/v1",
    "resolved_key_is_new_key": true
  },
  "final_base_url": "http://127.0.0.1:18082/v1",
  "wrong_address_new_key_pair_observed": true
}
~~~

이 재현의 HTTP 호출은 0건이다. 실제 키 유출 사고를 관찰했다는 뜻은 아니다. **이후 요청에 사용되는 Connection 자체가 새 키와 이전 주소를 묶어 반환됨**을 확인한 것이다. 이 상태로 모델 조회·연결 확인·생성을 시작하면 새 서버용 키가 이전 서버로 갈 수 있다. 리다이렉트 차단과는 별개다.

수정은 읽기와 쓰기 양쪽에 적용해야 한다. 키 저장만 기존 파일 쓰기 잠금 안으로 옮겨서는 충분하지 않다. 주소·종류·키 참조를 일관된 버전으로 읽는 공통 경로를 만들거나, 새 자격 증명을 별도 ID에 저장한 뒤 프로필의 불변 참조를 원자적으로 교체해야 한다. R10의 생성 상태도 같은 연결 버전과 묶는 것이 적절하다.

회귀 검사는 앱/CLI 수정과 resolve의 교차 실행을 고정해 이전 주소+새 키, 새 주소+이전 키 조합이 모두 불가능함을 확인해야 한다.

**코드 근거**

- E:\ai-image-generator\apps\studio\src-tauri\src\service\profiles.rs — 203~213행
- E:\ai-image-generator\crates\image-cli\src\main.rs — 430~432행
- E:\ai-image-generator\crates\image-core\src\connection.rs — 154~178행
- E:\ai-image-generator\crates\image-core\src\profiles.rs — update 및 쓰기 잠금 경로

**실행 증거:** [B1] profile_key_address_race.

### R2. Gemini HTTP 200 차단 응답은 키 제거를 거치지 않는다 — P2

HTTP 오류 상태는 키를 알고 있는 classify_http_error에서 정리하지만, HTTP 200에 담긴 Gemini 차단 결과는 extract_image에서 별도 오류로 변환된다. finishMessage와 blockReason이 들어가는 이 경로에 키 제거가 빠져 있다.

가짜 키를 fake-key-review-only로 설정하고, 가짜 Gemini 공급자가 같은 문자열을 finishMessage에 포함한 HTTP 200 응답을 보내게 했다. 실제 생성 서비스 결과에서 다음이 확인됐다.

- 가짜 공급자 요청 1건, 외부 API 요청 0건.
- 실패 summary에 가짜 키 문자열 존재.
- 진행 이벤트에 같은 문자열 존재.
- 실패 기록 JSON 1개에 같은 문자열 존재.

공식 Google 서비스가 실제로 키를 이 필드에 돌려준다는 주장은 아니다. **외부 오류 응답의 내용에 관계없이 키를 파일·로그에 남기지 않는다는 보장이 한 응답 경로에서 빠진 것**이다.

키를 알고 있는 produce 경계에서 extract_image 오류의 message와 provider_code를 정리하고, 기록·이벤트로 나가기 전 공통 보호를 적용해야 한다. 기존 non-200 테스트에 더해 200 차단 응답의 summary·progress·record를 검사해야 한다.

**코드 근거**

- E:\ai-image-generator\crates\image-core\src\adapter.rs — 330~349행
- E:\ai-image-generator\crates\image-core\src\generate.rs — 148~153행
- E:\ai-image-generator\crates\image-core\src\store.rs — 실패 기록 저장
- E:\ai-image-generator\crates\image-core\src\runner.rs — 실패 결과·진행 이벤트 구성

**실행 증거:** [B2] pipeline.

### R3. 이름이 env인 저장 프로필을 환경변수 연결로 처리한다 — P2

저장 프로필 이름 env는 앱과 Rust 검증에서 허용된다. 반면 생성 폼은 같은 문자열을 환경변수 연결의 식별자로 사용한다.

실제 connectionChoices와 toInput을 실행한 결과, 환경변수 연결과 저장 프로필의 선택값이 모두 env였고, 저장 프로필을 뜻하는 폼에서도 IPC 입력 profile이 null로 바뀌었다.

~~~json
{
  "choices": [
    {"value": "env", "label": "환경변수 연결"},
    {"value": "env", "label": "env · CLIProxyAPI"}
  ],
  "input_profile": null
}
~~~

null은 “이름이 env인 프로필”이 아니라 환경변수 또는 기본 프로필의 해석을 요청한다. 따라서 명시적으로 고른 서버·계정과 실제 호출 대상이 달라질 수 있다. 모델 목록 조회와 연결 검사 캐시에도 같은 문자열 충돌이 있다. 실제 계정에 요청을 보내지는 않았다.

선택값을 표시 이름에서 분리해야 한다. 저장 프로필은 안정적인 ID, 환경변수 연결은 별도 종류로 전달하고, 검사 캐시에도 같은 구분을 써야 한다. 단순히 새 이름만 금지한다면 기존 동명 프로필 처리도 필요하다.

**코드 근거**

- E:\ai-image-generator\apps\studio\src\lib\generateForm.ts — 19, 192~197, 233행
- E:\ai-image-generator\apps\studio\src\lib\generate.svelte.ts — 91행
- E:\ai-image-generator\apps\studio\src\lib\state.svelte.ts — 70행
- E:\ai-image-generator\apps\studio\src\lib\connectionsForm.ts — 223~247행
- E:\ai-image-generator\crates\image-core\src\profiles.rs — 이름 검증

**실행 증거:** [U1] env_named_profile.

### R4. 기록에서 재개하면 아직 시작하지 않은 작업이 사라진다 — P2

원래 요청 장수는 Started.total 이벤트에만 있고 영속 실행 명세에는 없다. 갤러리는 존재하는 개별 기록만 읽고, resumeFrom은 기록 개수와 파일명의 최고 번호로 새 요청 장수를 추정한다.

실제 모의 연결 UI에서 50장을 시작해 취소했다. 완료 기록은 13개, 최고 번호는 14였다. 기록 카드의 “이어서 생성”을 선택하자 폼과 버튼이 **14장 이어서 생성**으로 바뀌었다. 15~50번, 총 36장이 복원된 계획에서 빠졌다.

별도 Rust 서비스에서도 5장·동시성 1로 시작해 첫 가짜 요청에서 취소했다. Started.total=5와 summary 실패 5건은 존재했지만, 다시 읽은 갤러리 기록은 1개이고 원래 요청 장수 필드는 없었다.

같은 화면에 남아 있는 “남은 장 이어 만들기”는 메모리의 last.input을 사용하므로 구분해야 한다. 문제는 기록 카드·갤러리·재시작 후 복원 경로다.

또한 개별 기록에는 profile_id·connection_source가 있지만 GalleryItem과 복원 설정에서 제외된다. 기록의 재개는 현재 선택된 연결을 유지한다. 같은 모델을 제공하는 연결 A/B라면 A의 완료분을 재사용하면서 나머지만 B로 보낼 수 있다. 이 연결 부분은 소스 경로로 확인했으며 실제 다른 계정 호출은 하지 않았다.

공급자 호출 전에 topic별 실행 명세를 저장해야 한다. 요청 장수·예정 번호·입력 옵션·출력 루트·원래 연결을 보존하고, 재개는 이를 읽어야 한다. 별도 데이터베이스 없이 작은 JSON 명세로 구현할 수 있다. 목표 수가 없는 옛 기록은 추정값을 조용히 확정하지 말고 복원 한계를 표시해야 한다.

**코드 근거**

- E:\ai-image-generator\apps\studio\src\lib\generateForm.ts — 258~305행
- E:\ai-image-generator\apps\studio\src\lib\generate.svelte.ts — 245~279행
- E:\ai-image-generator\apps\studio\src-tauri\src\service\generate.rs — 321행
- E:\ai-image-generator\apps\studio\src-tauri\src\service\gallery.rs — GalleryItem 및 read_item
- E:\ai-image-generator\crates\image-core\src\store.rs — Record 및 request_hash

**실행 증거:** [B1] cancelled_run_manifest, [U1] history_resume, [U2], [U3], 이어하기 화면 캡처.

### R5. 참조 검사 중에도 참조 없이 생성할 수 있다 — P2

addReferencePaths는 검사 응답을 기다린 뒤에야 form.references에 추가한다. 검사 중 상태가 없고, 생성 버튼·Ctrl+Enter·startRun은 이 대기를 시작 금지 조건으로 사용하지 않는다. 기록 복원도 참조 목록을 비운 뒤 같은 검사를 기다린다.

실제 컨트롤러에 지연된 inspect_reference와 가짜 IPC를 제공해 실행했다.

~~~json
{
  "start_generation_called": true,
  "sent_reference_paths": [],
  "form_reference_paths_after_inspect": ["C:/review/reference.png"]
}
~~~

검사가 끝나 참조가 폼에 보이더라도 이미 시작한 요청에는 반영되지 않는다. 외부 API 요청은 0건이다.

참조 검사·설정 복원 중 상태를 두고 버튼, 단축키, 컨트롤러에서 공통으로 시작을 막아야 한다. 모델 목록을 불러오는 동안에도 이전 목록으로 시작할 수 있는 경로가 있으므로, 현재 연결에 해당하는 모델·참조 준비가 모두 끝났는지를 같은 게이트에서 확인하는 것이 적절하다. 모델 목록의 추가 경로는 코드 확인 사항이며 이번 별도 실행의 측정 대상은 참조 검사다.

**코드 근거**

- E:\ai-image-generator\apps\studio\src\lib\generate.svelte.ts — 80~108, 123~135, 171~178, 245~261행
- E:\ai-image-generator\apps\studio\src\screens\Generate.svelte — 단축키 및 생성 버튼 활성 조건

**실행 증거:** [U1] pending_reference.

### R6. 공급자만 구분하고 모델별 요청 크기 제한은 적용하지 않는다 — P2

실제 build_call 함수에 앱 기본 크기 1920×1080을 넣었다.

| 모델·연결 | 로컬 수락 | 구성된 공급자 크기 |
|---|---|---|
| Gemini 공식 / gemini-3.1-flash-lite-image | 수락 | imageSize = 2K |
| OpenAI 공식 / gpt-image-1.5 | 수락 | size = 1920x1088 |

Google 공식 문서는 Flash Lite Image가 1K만 지원한다고 명시한다. 이 모델에 2K를 구성하는 것은 현재 공식 계약과 맞지 않는다.

OpenAI 공식 참조는 임의 해상도 지원을 GPT Image 2·2.5 계열에 명시하고, 공통 표준 크기를 1024×1024·1536×1024·1024×1536으로 제시한다. 이전 GPT 모델까지 같은 패딩 규칙을 적용하는 부분도 모델별 크기 표에서 분리해야 한다. 대표 확정 사례는 명시적인 1K 제한이 있는 Gemini다.

요청 구성은 네트워크 없이 확인했다. 공식 서버에 잘못된 요청을 보내 실제 거부·과금을 측정한 것은 아니다. 공급자가 받을 크기와 최종 저장 크기를 분리해 지원 크기로 생성 후 맞추거나, 지원하지 않는 조합을 호출 전에 거부해야 한다.

**코드 근거**

- E:\ai-image-generator\crates\image-core\src\adapter.rs — 182~205행
- E:\ai-image-generator\crates\image-core\src\size.rs — 69~74행

**공식 근거, 검토일 기준**

- [Google 이미지 생성 문서](https://ai.google.dev/gemini-api/docs/image-generation) — “Flash Lite Image only supports 1K”와 해상도 절.
- [OpenAI Create image 참조](https://developers.openai.com/api/reference/resources/images/methods/generate) — size 항목.

**실행 증거:** [B2] gemini_lite_size_sent, older_openai_size_sent.

### R7. 손상 참조를 헤더만 검사해 공급자로 보낸다 — P2

inspect_reference와 엔진 validate_request 모두 into_dimensions로 헤더를 읽는다. 전체 이미지 디코딩이 가능한지 검사하지 않는다.

정상 170바이트 PNG를 41바이트로 잘랐을 때 헤더 검사는 성공하고 전체 디코딩은 실패했다. 실제 생성 서비스가 이 참조를 넣어 가짜 공급자에 1번 요청했고, 가짜 공급자가 정상 결과를 반환하자 서비스도 성공으로 끝났다.

이는 실제 과금이 발생했다는 증거가 아니다. 로컬에서 손상됐다고 판별할 수 있는 참조가 사전 검증을 통과해 API 호출까지 도달한다는 증거다.

바이트·픽셀·메모리 제한을 적용한 전체 디코딩을 호출 전에 수행해야 한다. 검증한 바이트를 그대로 요청에 사용하면 검사와 읽기 사이에 파일이 바뀌는 문제도 줄일 수 있다.

**코드 근거**

- E:\ai-image-generator\apps\studio\src-tauri\src\service\generate.rs — 125~130행
- E:\ai-image-generator\crates\image-core\src\generate.rs — 50~53행

**실행 증거:** [B1] truncated_reference.

### R8. 실제 불투명 기록을 “투명 0.0%”로 렌더한다 — P2

실제 일반 PNG 기록에는 alpha.transparent_ratio=0.0이 저장되고, 갤러리는 이를 그대로 0으로 전달한다. Viewer는 값이 null이 아니라는 이유로 배경을 “투명”이라고 출력한다.

실제 Viewer.svelte를 SSR로 렌더해 다음 차이를 확인했다.

| 입력 | 출력 |
|---|---|
| 실제 불투명 기록: transparentRatio=0, transparent=false | 배경 투명 / 완전 투명 0.0% |
| 기존 mock: transparentRatio=null, transparent=false | 배경 항목 없음 |
| 투명 기록: transparentRatio=0.62, transparent=true | 배경 투명 / 완전 투명 62.0% |

따라서 mock UI 검사만으로 놓치기 쉬운 실제/모의 데이터 계약 차이가 있다. 실제 WebView2 창에서 이 문구를 관찰한 것이 아니라 같은 컴포넌트를 렌더한 결과다.

요청한 배경, 알파 채널 유무, 완전 투명 픽셀 비율을 구분해야 한다. 0은 투명 픽셀 없음으로 표시하고, 반투명 픽셀만 있는 경우도 정확하게 다뤄야 한다. 모의 데이터에도 실제 Rust가 보내는 0.0을 넣어야 한다.

**코드 근거**

- E:\ai-image-generator\apps\studio\src\components\Viewer.svelte — 135~140행
- E:\ai-image-generator\apps\studio\src-tauri\src\service\gallery.rs — 213행
- E:\ai-image-generator\apps\studio\src\dev\mock-ipc.ts — 불투명 값 null 생성 경로

**실행 증거:** [U1] viewer, [A1] 실물 이미지 기록.

### R9. 인증 여부 미확인을 인증 실패로 표시한다 — P2

core의 check_connection은 404·5xx·리다이렉트에서 서버 응답은 확인하지만 키 승인 여부를 증명할 수 없으므로 authenticated=false를 반환한다. 프런트엔드는 reachable=true, authenticated=false를 모두 인증 단계 fail로 변환한다.

실제 stagesOf 함수에 404·503·302 보고서를 넣었고 모두 인증 단계가 fail이 됐다. 오류 설명 자체는 주소 경로 오류·제공자 오류·이동 요구로 보존된다. 따라서 “전부 키가 거부됐다고 설명한다”는 표현은 부정확하며, 문제는 **인증 실패가 확인되지 않았는데 인증 단계에 실패 표시를 붙이는 것**이다.

인증을 성공·거부·미확인으로 나누거나, 오류 분류를 이용해 키 거부가 확인된 경우만 fail로 표시해야 한다. 최신 Rust 결과 형식을 그대로 쓰는 계약 검사가 필요하다.

**코드 근거**

- E:\ai-image-generator\crates\image-core\src\check.rs — 59~64행
- E:\ai-image-generator\apps\studio\src\lib\connection.ts — 40~59행
- E:\ai-image-generator\apps\studio\src\lib\connection.test.ts — 일부 fixture가 최신 core 보고와 다름

**실행 증거:** [U1] connection_stages.

### R10. 연결을 바꾸고도 이전 서버의 생성 성공 상태가 유지된다 — P2

성공 GenerationStatus를 기록한 임시 프로필의 주소를 바꾼 뒤 실제 generation_status를 다시 읽었다. 새 주소에 생성 요청을 한 적이 없지만 같은 ID의 old success가 유지됐다. check와 ProfileView는 이 값을 화면에 전달한다.

상태를 profile ID만으로 저장하면 주소·키 수정이 새 연결로 바뀌었다는 사실을 표현하지 못한다. 주소·키 변경 시 현재 연결의 생성 확인을 미확인으로 되돌리거나 상태에 연결 revision을 포함해야 한다. 이전 revision에서 진행 중인 생성이 늦게 끝나 새 revision의 상태를 성공으로 바꾸지 않도록 해야 한다.

**코드 근거**

- E:\ai-image-generator\crates\image-core\src\profiles.rs — update 및 generation_status
- E:\ai-image-generator\crates\image-core\src\check.rs — 29~43행
- E:\ai-image-generator\apps\studio\src-tauri\src\service\profiles.rs — ProfileView 구성
- E:\ai-image-generator\apps\studio\src\lib\connection.ts — 61~76행

**실행 증거:** [B2] updated_profile_retains_old_success.

## 3. 이전 10개 지적의 수정 확인

이전 재현 스크립트는 별도 임시 홈·가짜 키·loopback 공급자로 다시 실행했다. 유효한 투명 옵션을 확인하는 옛 fixture가 불투명 이미지를 돌려주는 경우에는 새 투명 검사 때문에 실패하는 것이 맞으며, 옵션 전달 확인과 결과 검증을 분리했다.

| 이전 문제 | 이번 확인 |
|---|---|
| 결과·기록 저장 경쟁으로 다른 실행 결과를 건드림 | 예약 후 재검사, pending 쓰기 충돌 거부, finalize 해시 확인 및 정확한 교차 실행 회귀 테스트 통과 |
| batch transparent/fit 옵션 무시 | 실제 요청에 transparent 전달, 기록 fit 반영 확인 |
| 투명 요청에 불투명 결과를 성공 처리 | 실패 반환·failed 기록 확인 |
| 손상된 기록으로 resume | 충돌·실패 반환, 공급자 요청 0건 |
| reference_images 문자열 허용 | 구조 오류로 호출 전 거부 |
| batch count=0 허용 | 호출 전 거부 |
| 모델명으로 출력 경로 변경 | 호출 전 거부 |
| 과도한 timeout에서 panic | 구조화된 오류 반환, 공급자 요청 0건 |
| Rust의 ~ 경로 해석 차이 | Python/Rust dry run의 홈 확장 경로 일치 |
| 실패 목록 순서 불안정 | 반복 재현에서 번호 1,2,3 순서 확인 |

저장 경쟁 회귀는 E:\ai-image-generator\crates\image-core\tests\store.rs의 run_that_inspected_before_another_completed_cannot_take_its_output으로 확인했다. 나머지 재현 원본은 [G1], [G2]에 있다.

기록 없는 옛 이미지·상태 필드 없는 Python 기록에 대한 명시적 호환 정책은 새 결함으로 다시 세지 않았다.

## 4. 재실행한 검증

| 검증 | 결과 |
|---|---|
| cargo test --workspace --offline --locked, 격리 target | 212 통과, 0 실패, 2 ignored |
| Python/Rust CLI parity | 통과, Rust 수치에 포함 |
| Python unittest discover | 39 통과 |
| pnpm test | 9파일, 프런트엔드 단위테스트 132 통과 |
| pnpm check | Svelte 0 errors / 0 warnings, TypeScript 통과 |
| pnpm build | 통과 |
| cargo clippy --workspace --all-targets --offline --locked -- -D warnings | 통과 |
| 독립 backend probe | 세 경로 재현, 종료 코드 0 |
| 독립 adapter/응답 probe | 요청 크기·키 노출·과거 성공 보존 재현, 종료 코드 0 |
| UI 계약 probe | 폼·컨트롤러·상태 함수 및 Viewer 렌더 재현, 종료 코드 0 |

Rust의 ignored 2개는 실제 프록시를 사용하는 명시적 smoke 테스트다. 이번에 실행하지 않았다. 132개 프런트엔드 테스트는 대부분 순수 함수·상태·명령 계약 검사이며 실제 .svelte 화면 mount/render 테스트 132개를 뜻하지 않는다.

처음 일반 target으로 실행한 Rust 빌드는 사용자가 실행 중인 studio.exe 때문에 Windows 파일 접근 오류가 났다. 앱을 종료하지 않고 E:\ai-image-generator\target\review-stage23-20261019로 빌드를 격리한 뒤 위 테스트를 끝냈다. 이것은 제품 테스트 실패와 구분한다.

SSR 진단의 첫 시도는 진단 코드가 서로 다른 Svelte 런타임 인스턴스를 섞어 렌더에 실패했다. 같은 Vite SSR 그래프의 render를 쓰도록 진단만 고쳐 성공했다. 첫 시도 자료도 보존했으며 제품 결함으로 세지 않았다.

### 실제 화면에서 확인한 범위

기존 Vite 서버의 http://127.0.0.1:1420/?mock=1에서 실제 Svelte 화면을 Playwright로 조작했다. 6장 진행·재시도·실패 카드, 50장 실행 중 화면 전환, 취소, 기록 재개, 연결 확인, 새 연결 종류 선택, 갤러리·투명 뷰어를 확인했다. ?mock=empty에서는 첫 연결 안내와 빈 입력 거부를 확인했다.

이 경로는 mock IPC다. 실제 Windows 창의 IPC·파일 선택 대화상자·닫기·WebView2 표시까지 검증한 결과로 확대해서 해석하면 안 된다. RDC의 native desktop provider는 비활성 상태여서 실제 창 조작은 실행하지 않았다. 검토용 브라우저 탭은 닫았고 기존 Studio와 Vite 서버는 유지했다.

## 5. 기존 실물 이미지 확인

아래 2개를 직접 열고 디코딩·크기·알파·SHA-256을 검사했다. 기록의 해시가 파일과 일치한다.

| 파일 | 저장 크기·형식 | 투명 비율 | 기록상 요청 모델 |
|---|---|---:|---|
| E:\ai-image-generator\ImageGallery\output\stage2-profile-check\stage2-profile-check.png | 1024×1024 RGB | 0% | gpt-image-2.5 |
| E:\ai-image-generator\ImageGallery\output\studio-app-smoke-20261018\studio-app-smoke-20261018.png | 1024×1024 RGB | 0% | gpt-image-2.5 |

첫 이미지는 흰 배경의 파란 원, 두 번째는 햇빛이 들어오는 책상 위 찻잔이며 기록의 프롬프트와 시각적으로 맞는다. 두 기록의 background는 auto, quality는 low다. 두 번째 기록에는 local 프로필 ID가 있다. 내부 모델 변종은 기록에 보고되지 않아 확인할 수 없다.

두 장은 불투명 결과다. 이 파일 존재 확인이나 앱 서비스 smoke는 실제 앱 창의 투명 생성·취소·재개 검증을 대신하지 않는다. 이번 검토에서는 새 유료 이미지를 만들지 않았다.

## 6. 알려진 선택과 남은 검증

다음은 첨부에 공개된 선택이므로 새 숨은 결함으로 세지 않았다.

- 투명 요청에 불투명 응답이 오면 실패시키고 받은 이미지는 공개 저장하지 않음.
- Gemini 프로필 주소의 /v1beta 입력을 거부함.
- 짧은 머신 이름의 plain HTTP에 경고를 생략함.
- 앱 생성 장수는 계획대로 1~50, CLI batch 상한은 1000.
- 앱은 디버그 실행 파일 단계이며 서명·설치 파일 작업은 뒤 단계.
- 현재 모델 이름 규칙이 허용하는 내장 모델 범위만 지원하며, 각 공급자의 모든 최신 모델을 자동 지원하는 것은 아님.

출시 전의 중요한 미검증 영역은 실제 Windows 앱의 생성→취소→재개→종료, 공식 OpenAI/Gemini/xAI 직접 연결, 노트북→데스크톱 Tailscale 경로다. 이번에는 이 실환경 경로를 실행하지 않았다. 검토 시작 시 노트북은 오프라인이었다.

투명 실패 응답을 나중에 배경 제거로 재사용하려면 임시 원본 보존과 실패/완료 상태 분리를 별도 설계해야 한다. 단지 파일을 성공 갤러리에 넣는 것으로 해결하면 기존의 투명 성공 보장을 깨뜨린다.

## 7. 증거와 재현 파일

모든 새 원격 진단 자료의 루트는 다음과 같다.

~~~text
E:\ai-image-generator\ImageGallery\metadata\stage23-review-20261019\
~~~

| 증거 ID | 파일·핵심 내용 |
|---|---|
| B1 | backend-probe-results.json — 프로필 경쟁, 취소 manifest, 손상 참조 |
| B2 | adapter-contract-probe-results.json — 모델 크기, Gemini 오류 정제, 상태 보존 |
| U1 | ui-contract-probe-results.json — env 충돌, 검사 대기, 투명 렌더, 인증 상태 |
| U2 | ui-cancelled-50.txt — 50장 취소 화면 snapshot |
| U3 | ui-history-resume-count.txt — 14장으로 바뀐 폼 snapshot |
| A1 | assets-and-state.json — 두 실물 파일·기록·해시·알파 |
| G1 | offline-probe-results.json — 기존 재현 회귀 |
| G2 | contract-followup-results.json — 경로·입력 계약 회귀 |
| T1 | rust-isolated-verification.json, rust-isolated-1.log |
| T2 | python-verification.json, python-1.log |
| T3 | frontend-verification.json, frontend-1.log~frontend-3.log |
| T4 | clippy-verification.json, clippy-1.log |

진단 소스는 같은 루트의 backend-probe\Cargo.toml, backend-probe\src\main.rs, backend-probe\src\adapter_contract_probe.rs, ui_contract_probe.mjs, regression_probe.py, contract_regression_probe.py, verification_runner.py, verification_isolated.py, assets_audit.py에 있다.

화면 캡처도 같은 루트에 있다.

- stage23-review-20261019-generate.png
- stage23-review-20261019-resume-count.png
- stage23-review-20261019-gallery-transparent.png

진단 Cargo의 경로 의존성은 E:\ai-image-generator를 가리킨다. 저장소 루트에서 재현할 수 있다. 새 유료 호출은 하지 않고 임시 홈·가짜 키·loopback 공급자를 쓴다.

~~~powershell
cargo run --offline --manifest-path ImageGallery\metadata\stage23-review-20261019\backend-probe\Cargo.toml --bin stage23-backend-review-probe --target-dir E:\ai-image-generator\target\review-stage23-20261019 --quiet -- E:\ai-image-generator\ImageGallery\metadata\stage23-review-20261019\backend-probe-results.json

cargo run --offline --manifest-path ImageGallery\metadata\stage23-review-20261019\backend-probe\Cargo.toml --bin adapter-contract-probe --target-dir E:\ai-image-generator\target\review-stage23-20261019 --quiet -- E:\ai-image-generator\ImageGallery\metadata\stage23-review-20261019\adapter-contract-probe-results.json

node ImageGallery\metadata\stage23-review-20261019\ui_contract_probe.mjs
~~~

## 8. 권장 수정 순서와 종료 조건

1. R1의 일관된 연결 버전과 R2의 모든 오류 경로 키 정제를 먼저 수정한다. R10 상태도 연결 버전과 묶는다.
2. R3의 연결 ID 충돌과 R4의 실행 명세 저장을 고친다. 재시작 후 원래 장수·연결·참조로 재개하는 회귀 검사를 둔다.
3. R5의 입력 준비 게이트, R6 모델별 기능표, R7 참조 전체 검증을 호출 이전 공통 경로에 적용한다.
4. R8 투명 표시와 R9 인증 미확인 상태를 고치고 실제 Rust DTO로 소수의 컴포넌트·계약 검사를 추가한다.
5. 기존 테스트와 위 결함에 대응하는 회귀 검사를 통과시킨 뒤 실제 Windows 창 검증을 수행한다. 각 공식 API의 실호출 및 노트북 검증 결과는 별도로 기록한다.

검토 시 제품 소스·프로필·실제 키·환경변수·커밋은 변경하지 않았다. 진단 파일과 이 리뷰 문서만 추가했으며 커밋·푸시는 수행하지 않았다. 문서 작성 직전에도 HEAD는 위 6cb4f62이고 git status는 깨끗했다.
