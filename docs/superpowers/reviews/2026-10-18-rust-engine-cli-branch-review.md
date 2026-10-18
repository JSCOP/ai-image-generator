# Rust 엔진·CLI 브랜치 리뷰 — 2026-10-18

## 판정

**수정 후 재검토가 필요합니다. P1 3건, P2 7건을 확인했습니다.** 기존 자동 테스트와 정상 생성 결과는 통과했지만, 저장 결과의 요청 일치, 실제 투명성, 배치 옵션 전달에 완료 판정을 막는 결함이 있습니다. Python CLI와의 JSON 호환성도 아직 모든 확인 사례에서 성립하지 않습니다.

이 문서는 코드 리뷰 결과입니다. 구현 코드 수정, 커밋, 병합, 설정 변경, 새로운 유료 이미지 생성은 수행하지 않았습니다. 진단 스크립트와 결과, 이 보고서만 추가했습니다.

## 1. 검토 기준과 범위

- 장치 / 저장소: `jisung-desktop` / `E:\ai-image-generator`.
- 브랜치: `feat/rust-engine-cli`. 설계·외부 검토·단계 0–1 계획의 기준 main은 `5cc6b2b8a85344d336a26f1b38ec1de8f600a144`.
- 자동 테스트 및 핵심 소스 대조 기준: **`27e6a02e10a77c5f05d7c49f4d126f21e43a94f1`**. `fdf7438`의 잠금·중단 복구·패닉 결과 처리 수정도 포함합니다.
- 검토용으로 읽은 소스·테스트·계획 39개 파일의 SHA-256이 위 커밋 시점의 실제 작업 트리와 모두 일치했습니다. 당시 작업 트리는 깨끗했습니다.
- 검토 도중 `ea6a3b5436954b9e63ff4d7378095a20ecbebf65`(Windows 자격 증명 저장소)가 추가됐습니다. 이 커밋은 아래 결함 관련 코드를 변경하지 않았습니다.
- 보고서를 저장한 뒤 최종 대조에서는 `4ea816c`(프로필별 인증 연결)까지 진행됐습니다. 아래 발견 사항과 관련된 기존 파일의 변경은 인증 인자 전달·문자열 소유권 처리뿐이며, 10건의 원인 코드는 유지되어 있음을 확인했습니다. 자동 테스트 76개라는 수치를 이 추가 단계 전체의 실행 검증으로 해석해서는 안 됩니다.
- 후속 CLI 진단 시 Git HEAD는 `ea6a3b5`였지만 실행 파일은 앞서 테스트한 빌드입니다. 실행 파일 경로·수정 시각은 `offline-probe-results.json`에 기록했습니다.
- 이후 연결 프로필과 다음 단계 계획이 작업 트리에서 작성되고 있었습니다. 진행 중인 2단계 전체의 테스트·통합 완료를 이 리뷰가 보증하지는 않습니다.
- 주요 기준 문서: `E:\ai-image-generator\docs\superpowers\plans\2026-10-18-image-studio-stage0-1-engine-cli.md`, `E:\ai-image-generator\docs\RUST_PARITY.md`.

## 2. 직접 확인한 결과

| 검증 | 결과 | 범위 |
|---|---|---|
| `cargo test --workspace --offline --locked` | **Rust 76개 통과** | Windows 저장소에서 실행. CLI 동등성 테스트 포함 |
| `.venv\Scripts\python.exe -m unittest discover -s tests -v` | **Python 39개 통과** | 실제 저장소의 가상환경에서 실행 |
| `rust_cli_matches_python_cli` | 통과 | Python 실행을 생략하지 않고 공통 가짜 공급자로 비교 |
| Python 리다이렉트 추가 진단 | **10개 조합 모두 통과** | GET/POST × 301·302·303·307·308, 이동 대상 요청 0건 |
| 실제 Windows CLI 추가 진단 | 아래 결함 재현 | 로컬 가짜 공급자와 임시 이미지 사용, 실제 API 호출 0건 |
| 저장 함수 교차 순서 진단 | 이미지/기록 오결합 재현 | 실제 Rust 공개 저장 함수 호출, 외부 API 호출 없음 |
| 기존 실생성 이미지 2장 | 파일·기록·시각 검토 완료 | 두 이미지 모두 1024×1024, SHA-256과 기록 일치 |

통과한 테스트는 정상 경로의 근거입니다. 현재 동등성 테스트는 정상 single/jobs, dry-run, 일부 실패와 resume 등을 비교하지만, 이번에 발견한 배치의 새 옵션, 여러 실패의 순서, 잘못된 참조 타입, 숫자 경계, 홈 경로 확장까지 검증하지 않습니다.

### 기존 실생성 파일

| 항목 | 일반 이미지 | 투명 이미지 |
|---|---|---|
| 요청 모델 | `gpt-image-2.5` | `gpt-image-2.5-sunburst` |
| 저장 형식 / 크기 | RGB PNG / 1024×1024 | RGBA PNG / 1024×1024 |
| 완전 투명 픽셀 | 0 | **586,305 / 1,048,576 = 55.9144%** |
| 네 모서리 알파 | 255, 255, 255, 255 | **0, 0, 0, 0** |
| 기록 상태 | complete | complete |
| 기록된 fit | cover | contain |
| 이미지 해시와 기록 | 일치 | 일치 |
| 응답에 기록된 실제 모델 | `null` | `null` |

두 파일을 직접 열었습니다. 투명 이미지의 컵 손잡이는 보존되어 있습니다. 위 두 파일의 정상 결과와 별개로, 공급자가 불투명 결과를 돌려줄 때 CLI가 실패를 감지하지 못하는 문제는 F3으로 재현했습니다.

파일 위치:

- `E:\ai-image-generator\ImageGallery\output\rust-cli-smoke-20261018\rust-cli-smoke-20261018.png`
- `E:\ai-image-generator\ImageGallery\output\rust-cli-smoke-transparent-20261018\rust-cli-smoke-transparent-20261018.png`

응답 모델 식별자가 없으므로 실제 내부 변형 모델까지 확인했다는 주장은 할 수 없습니다. 이번 리뷰에서는 기존 파일을 검사했으며 새 실생성 요청을 보내지 않았습니다.

## 3. P1 — 병합 전에 먼저 수정할 사항

### F1. 잠금을 얻기 전의 검사 결과로 다른 요청의 완료 기록을 덮어쓸 수 있음

**위치:** `E:\ai-image-generator\crates\image-core\src\generate.rs` 100·119·135행; `E:\ai-image-generator\crates\image-core\src\store.rs` 248–253·262–264·289–317행.

`generate_one`은 기존 파일을 검사한 뒤 잠금을 획득합니다. 그 사이 다른 실행이 완료돼도, 잠금을 획득한 실행은 상태를 다시 확인하지 않고 자신의 pending 기록을 씁니다.

재현한 순서:

1. 요청 A가 같은 출력 위치를 검사해 `Absent`를 얻습니다.
2. 요청 B가 잠금을 얻어 이미지 B와 complete 기록 B를 저장하고 잠금을 해제합니다.
3. 요청 A가 잠금을 얻은 뒤 pending 기록 A로 기록 B를 덮어씁니다.
4. A의 이미지 공개는 기존 이미지 때문에 `conflict`로 실패합니다. 하지만 이미지 B와 pending 기록 A가 남습니다.
5. A를 resume하면 이미지 B를 A의 complete 결과로 확정합니다. 이미지 B의 해시까지 새 기록에 써 넣기 때문에 이후 해시 검사도 통과합니다.

**실제 재현 결과:** `a_publish_error_category="conflict"`, `a_request_can_finalize_b_image=true`, 최종 기록은 요청 A이고 최종 이미지·SHA-256은 B였습니다.

두 CLI 프로세스의 무작위 동시 실행을 관찰한 것은 아닙니다. 실제 `generate_one`이 사용하는 공개 저장 함수를 별도 Rust 진단 프로그램에서 위 순서로 호출해, 함수 사이에 허용되는 교차 순서를 결정적으로 재현했습니다.

**영향:** 다른 프롬프트의 이미지가 요청한 결과로 표시되고 재사용될 수 있습니다. 두 파일이 각각 유효하고 해시도 맞으므로 단순 파일 검증으로는 발견되지 않습니다. 실제 실행에서는 최종 충돌을 알기 전에 A의 생성 비용도 발생할 수 있습니다.

**수정:** 출력 잠금 안에서 이미지·기록·요청 일치를 다시 확인한 뒤 pending 기록과 생성 요청으로 진행해야 합니다. 복구 확정 경로에도 같은 잠금 안에서 재검사가 필요합니다. 이미 완료된 다른 실행의 기록은 보존해야 합니다.

**최소 회귀 테스트:** A가 최초 검사 후 멈추고 B가 완료한 뒤 A가 재개하는 순서를 고정합니다. B의 이미지·기록이 유지되고 A의 추가 공급자 요청이 0건인지 검증합니다.

**근거:** `storage-race-result.json`, `storage-probe\storage_review_probe.rs`.

### F2. batch가 background·fit 옵션을 버리고 잘못된 값도 검증하지 않음

**위치:** `E:\ai-image-generator\crates\image-core\src\batch.rs` 303–315행.

batch의 `ImageFields`가 `background: None`, `fit: None`을 고정합니다.

실제 CLI에 다음 옵션을 넣었습니다. 나머지는 진단용 topic_root와 로컬 공급자 설정입니다.

```json
{"mode":"batch","preset":"presets/extraction-rpg.json","count":1,"image_model":"gpt-image-2.5-sunburst","background":"transparent","fit":"contain"}
```

**실제 결과:** 공급자 요청 1건, 요청 본문에 background 없음, 저장 기록은 `background=auto`·`fit=cover`, 결과는 불투명인데 `ok=true`. `background="rainbow"`·`fit="stretch"`도 공급자 요청 1건 후 성공했습니다.

**영향:** 투명 소재를 배치로 요청해도 다른 옵션으로 비용을 지불하며 생성합니다. 지원하지 않는 옵션을 사전에 거부한다는 완료 설명도 이 경로에는 성립하지 않습니다.

**수정:** single/jobs와 같은 옵션 전달·검증을 batch에도 적용합니다. 이 단계에서 batch 지원을 의도하지 않았다면 명시적으로 호출 전에 거부해야 합니다.

**최소 회귀 테스트:** 유효한 옵션이 공급자 본문과 기록에 반영되는지, 잘못된 값은 공급자 요청 0건인지 확인합니다.

**근거:** `offline-probe-results.json`의 `batch_transparent_options`·`batch_invalid_options`.

### F3. 투명 요청에 완전히 불투명한 응답을 받아도 성공 처리함

**위치:** `E:\ai-image-generator\crates\image-core\src\generate.rs` 178–192행; `E:\ai-image-generator\crates\image-core\src\normalize.rs`의 정규화·알파 통계 처리.

`background="transparent"`로 64×64 이미지를 요청하고 가짜 공급자가 같은 크기의 불투명 RGB PNG를 반환하게 했습니다.

**실제 결과:** 요청 본문에는 transparent가 있었지만, CLI는 종료 코드 0·`ok=true`·기록 `complete`를 반환했습니다. 저장 기록의 `has_alpha_channel=false`, `transparent_ratio=0`, `opaque_ratio=1`이었습니다.

알파 통계를 기록할 뿐 요청 성공 여부를 판단하는 데 사용하지 않습니다.

**영향:** 투명 결과가 필요한 자동화에서 불투명 이미지를 정상 결과로 가져갈 수 있습니다. “실제 투명 정보가 없으면 실패”라는 설계 조건에 어긋납니다.

**수정:** 네이티브 투명 요청은 공급자가 반환한 원본과 저장 결과의 실제 알파를 검증해야 합니다. 불투명 원본에 contain 패딩을 추가해 생긴 투명 여백만으로 네이티브 투명 성공을 판정해서도 안 됩니다. 기대를 충족하지 못하면 구조화된 실패를 반환하고 complete로 저장하지 않아야 합니다.

**최소 회귀 테스트:** 같은 크기 불투명 RGB, 전부 불투명한 RGBA, 비정사각형 불투명 원본에 contain 패딩을 붙이는 경우를 거부하는지 확인합니다. 실제 투명 응답은 성공해야 합니다.

**근거:** `offline-probe-results.json`의 `transparent_opaque_response`.

## 4. P2 — 입력 검증과 호환성 수정

| ID | 결함 | 실제 재현 결과 | 수정 방향 |
|---|---|---|---|
| F4 | 손상된 기존 기록을 legacy 결과로 취급 | 정상 생성 후 기록만 `{broken`으로 바꾸고 프롬프트를 변경해 resume: 새 요청 0건, 기존 이미지 재사용, `ok=true` | 파일 없음·유효한 Python 기록·읽기/파싱 실패를 구분하고 손상 기록은 오류 처리 |
| F5 | `reference_images`의 잘못된 타입을 조용히 버림 | 문자열 `"missing.png"`을 주면 참조 없는 생성 요청 1건 후 성공 | 참조 목록의 타입을 사전에 검사하고 오류 반환 |
| F6 | batch `count:0`이 1장으로 바뀜 | 0장 입력에도 생성 요청 1건, `ok=true` | batch는 기존 Python처럼 1 미만을 오류 처리 |
| F7 | Gemini 모델 문자열이 HTTP 요청 경로를 바꿀 수 있음 | `gemini-image/../../../admin/restart#` 입력 시 가짜 서버에 `/admin/restart` POST 1건 | 모델 ID를 검증하고 URL의 한 경로 요소로 안전하게 구성 |
| F8 | 큰 timeout 입력에서 JSON 오류 대신 panic | `job_timeout_sec:1e100` + dry-run: 요청 0건, stdout JSON 없음, 종료 코드 **101** | 유한값·허용 범위 검증과 실패 가능한 Duration 변환 사용 |
| F9 | 홈 경로 `~`를 확장하지 않음 | `~/Pictures`가 Rust에서는 저장소 아래 `~\Pictures`, Python에서는 사용자 사진 폴더 | topic_root·출력·참조 경로에 기존 홈 확장 규칙 적용 |
| F10 | single 실패 배열의 순서가 Python과 다름 | 3장 동시 실패의 index가 **[2,1,3]** | single의 failures를 index로 정렬 |

### F4: 손상 기록과 의도된 legacy 예외를 구분해야 함

**위치:** `E:\ai-image-generator\crates\image-core\src\store.rs` 109–113행.

파일 읽기 실패와 JSON 파싱 실패가 모두 `None`으로 바뀌고, status가 없으면 유효한 이미지라는 이유만으로 재사용합니다. 진단에서는 원래 Rust가 쓴 complete 기록을 임시 테스트 폴더 안에서 깨뜨린 뒤 다른 프롬프트로 resume했습니다.

단계 0–1 계획 466–473행은 **기록이 없거나 status 없는 Python 기록을 가진 이미지의 재사용**을 호환성 예외로 명시합니다. 그 예외 자체를 미구현 결함으로 판단하지 않았습니다. 다만 파싱조차 불가능한 기존 기록까지 예외로 처리하면 요청 일치 검증을 우회합니다.

완료 설명의 “resume은 항상 요청 일치를 확인한다”도 이 legacy 예외가 있다는 조건을 붙여야 정확합니다.

**근거:** `offline-probe-results.json`의 `corrupt_record_resume`.

### F5·F6: 잘못된 입력이 비용 발생 요청으로 바뀜

**위치:** `E:\ai-image-generator\crates\image-core\src\runner.rs` 314–315행; `E:\ai-image-generator\crates\image-core\src\batch.rs` 280행.

`references()`는 배열 변환 실패를 빈 배열로 취급합니다. `reference_images:"missing.png"`의 잘못된 JSON 타입이 오류 없이 일반 생성으로 바뀌었습니다.

batch는 `count.max(1)`로 0을 1로 바꿉니다. 기존 `E:\ai-image-generator\scripts\gen_batch.py` 184–186행은 count가 1 미만이면 오류를 반환합니다. single에 있는 기존 최소 1장 동작까지 문제라고 판단한 것은 아닙니다.

**근거:** `offline-probe-results.json`의 `reference_images_string`·`batch_zero_count`.

### F7: 모델 ID의 경로 삽입

**위치:** `E:\ai-image-generator\crates\image-core\src\catalog.rs` 12–16행; `E:\ai-image-generator\crates\image-core\src\provider.rs` 128행.

모델의 접두어와 image 포함 여부만 검사한 뒤 URL에 직접 삽입합니다. URL 정규화 과정에서 `/../`가 상위 경로를 제거하고 `#` 뒤의 생성 메서드 부분이 fragment가 됩니다.

가짜 서버에서만 실험했습니다. 최종 CLI는 응답 형식 오류로 실패했지만, 잘못된 경로의 요청은 이미 1건 발생했습니다. 확인된 위험은 **설정된 서버의 다른 경로로 인증된 POST 요청이 전송될 수 있다는 것**입니다.

**근거:** `offline-probe-results.json`의 `gemini_model_path`.

### F8: timeout 범위 오류

**위치:** `E:\ai-image-generator\crates\image-core\src\runner.rs` 258–267행.

`Duration::from_secs_f64`에 범위를 검사하지 않은 숫자를 전달합니다. `1e100`은 JSON 숫자로 읽히지만 Duration으로 바꿀 수 없어 프로세스가 panic합니다. dry-run에서도 발생했습니다.

**근거:** `offline-probe-results.json`의 `timeout_overflow`. stderr에 `cannot convert float seconds to Duration: value is either too big or NaN`이 기록돼 있습니다.

### F9·F10: 실제 Windows에서 확인한 출력 계약 차이

**위치:** `E:\ai-image-generator\crates\image-core\src\runner.rs` 239–241·397–413행. 비교 대상은 `E:\ai-image-generator\tools\ai_image.py` 198–202·403–407행.

같은 `topic_root:"~/Pictures"`의 dry-run 결과:

- Rust: `E:\ai-image-generator\~\Pictures\ImageGallery\output\path-review`
- Python: `C:\Users\js\Pictures\ImageGallery\output\path-review`

실패 순서는 3장·동시성 3·재시도 0에서 가짜 공급자 응답 시간을 달리해 `[2,1,3]`을 얻었습니다. Python은 실패 배열을 index로 정렬합니다. 처음 실행한 다른 응답 지연 패턴에서는 우연히 `[1,2,3]`이었으므로, 최종 근거는 후속 진단 파일의 결과를 사용합니다.

**근거:** `contract-followup-results.json`.

## 5. 이미 반영된 결정 중 타당한 부분

### 배치에서 20개 실패 후 미시작 작업을 failures에 포함하는 정책

미시작 작업을 `cancelled`로 구분해 결과에 남기는 결정은 타당합니다. 요청한 작업이 결과에서 조용히 사라지지 않습니다. 현재 batch 테스트는 25개 계획·동시성 1에서 공급자 요청 20건과 전체 실패 25건을 확인합니다.

다만 의미는 **최종 실패 작업 20개 이후 새 작업을 시작하지 않는다**입니다. 재시도와 이미 진행 중인 작업이 있으므로 실제 API 요청 총수를 20건으로 제한하는 기능은 아닙니다. UI나 요약에서는 “실행 실패”와 “미시작 취소”를 별도로 집계하면 혼동을 줄일 수 있습니다.

**근거:** `E:\ai-image-generator\crates\image-core\src\runner.rs`의 실행 루프, `E:\ai-image-generator\crates\image-core\tests\batch.rs`.

### 연결 주소를 명시적으로 설정하는 결정

확인 당시 Windows 사용자 환경변수는 `CLIPROXY_BASE_URL=http://127.0.0.1:8317/v1`였습니다. 프록시 키는 설정 존재 여부만 확인했습니다. 이 설정과 키를 이번 리뷰에서 변경하지 않았습니다.

노트북 연결은 확인하지 못했습니다. 검토 시 연결 장치 목록에서도 노트북은 오프라인이었습니다. 단계 0–1 계획 179행은 노트북 확인을 나중에 사용자와 진행할 수 있고 Rust 작업을 막지 않는다고 명시하므로, 이 상태를 코드 결함이나 무단 단계 생략으로 분류하지 않았습니다. 노트북 → 데스크톱 실생성 검증은 별도 미완료 항목으로 남습니다.

## 6. 검증 범위의 경계

- 기존 실제 결과로 확인한 범위는 CLIProxyAPI를 통한 GPT 계열 일반 이미지 1장·투명 이미지 1장의 산출물입니다. 모든 공식 API 직접 연결, 모든 모델, 실제 편집 호출까지 검증한 것은 아닙니다.
- 참조 편집은 기존 테스트의 가짜 공급자 시나리오로 확인됩니다. 실제 마스크 밖 픽셀 보존, 로컬 배경 제거, Tauri/Svelte 화면, 설치 파일, Python·MCP 전체 이관은 이 단계 0–1 완료 판정의 범위가 아닙니다.
- `ea6a3b5`의 `secrets.rs`와 `tests/secrets.rs`는 추가로 소스 검토했습니다. 키 trim·빈 값/길이 거부, Debug 마스킹, service/profile별 저장, 없는 항목 처리, 비Windows 오류 반환에서 명백한 병합 차단 문제를 찾지 못했습니다.
- 위 자격 증명 추가분의 테스트나 Windows 자격 증명 관리자 실제 호출을 이번 리뷰에서 수행하지 않았습니다. 프로필 선택·주소와 키의 결합·CLI 통합은 진행 중인 다음 단계의 검증 대상입니다.

## 7. 권장 수정 순서와 완료 조건

1. F1과 F4부터 수정해 이미지·기록·요청의 관계를 지킵니다. 잠금 안의 재검사와 손상 기록 구분을 회귀 테스트로 고정합니다.
2. F2와 F3을 수정해 투명 요청이 옵션 전달부터 저장 결과 검증까지 일관되게 동작하도록 합니다.
3. F5–F8은 잘못된 입력의 공급자 요청 수 0건과 구조화된 오류를 확인합니다.
4. F9–F10은 Python과 동일한 경로 및 실패 순서를 비교하는 동등성 사례에 추가합니다.
5. 수정된 고정 커밋에서 Rust/Python 전체 테스트와 이번 결함의 표적 회귀 테스트를 실행한 뒤 병합 여부를 판단합니다.

P1 수정 전에는 저장·투명 동작이 보장되는 완료본으로 판단하기 어렵습니다. P2 계약 차이도 수정하거나 명시적으로 승인된 예외로 문서화하기 전에는 “Python과 같은 JSON 계약”을 전 범위에서 충족했다고 표시하면 안 됩니다.

## 8. 증거와 테스트 파일 위치

이 보고서:

`E:\ai-image-generator\docs\superpowers\reviews\2026-10-18-rust-engine-cli-branch-review.md`

이번 리뷰의 진단 자료 폴더:

`E:\ai-image-generator\ImageGallery\metadata\rust-branch-review-20261018`

| 파일 | 내용 |
|---|---|
| `offline_branch_probe.py` | 실제 Windows Rust CLI를 로컬 가짜 공급자에 연결하는 진단 |
| `offline-probe-results.json` | 입력·요청 경로·횟수·CLI 결과·알파 통계·손상 기록 재현 |
| `contract_followup_probe.py` | 실패 순서와 Windows 홈 경로의 후속 비교 |
| `contract-followup-results.json` | 최종 실패 순서 및 Python/Rust dry-run 경로 |
| `storage-probe\storage_review_probe.rs` | 실제 저장 함수의 교차 순서 재현 |
| `storage-probe\Cargo.toml` | 저장 진단용 독립 Cargo 프로젝트 |
| `storage-race-result.json` | 이미지 B가 요청 A의 결과로 확정된 재현 결과 |
| `verify_runtime.py` | 소스 해시·환경 설정 존재·기존 이미지 측정 |
| `runtime-verification.json` | 커밋·39개 소스 해시·실제 이미지 2장의 측정값 |
| `test-summary.json` | 전체 테스트 개수·명령·검토 커밋 요약 |
| `python-redirect-matrix.json` | Python 리다이렉트 10개 조합의 별도 검증 |

가짜 공급자가 만든 진단용 이미지는 임시 폴더에서 사용한 뒤 정리됩니다. 다시 볼 실제 이미지 2장은 앞서 적은 `ImageGallery\output` 경로에 그대로 있습니다. 보고서는 커밋하지 않았습니다.
