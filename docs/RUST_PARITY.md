# Rust `image-cli`와 Python `tools/ai_image.py`의 의도된 차이

`crates/image-cli/tests/parity.rs`는 같은 가짜 제공자에 같은 spec을 보내고 두 CLI의 결과 JSON, 파일 목록, 이미지 크기, 기록 JSON, 제공자가 받은 요청 본문을 비교합니다. 아래 항목만 비교에서 빼며, 이 목록은 테스트의 `DROPPED_KEYS`·`DROPPED_FAILURE_KEYS`와 같아야 합니다. 설계 근거는 `docs/superpowers/specs/2026-10-18-image-studio-rust-app-design.md`입니다.

| 차이 | Rust 동작 | 이유 (설계 절) |
|---|---|---|
| 머신별 기본 주소 | 연결은 spec `profile`/`--profile` → 프로세스 `CLIPROXY_BASE_URL` → Windows 사용자 `CLIPROXY_BASE_URL` → 기본 프로필 순서로 정하고, 모두 없으면 네트워크 호출 전에 오류. `DESKTOP-SB818KQ`/Tailscale 주소 자동 선택 없음 | 배포본이 소유자 PC로 요청하지 않게 (§5.2) |
| Responses API 경로 | 옮기지 않음. `gpt-image-*`, `gemini-*-image`, `grok-imagine-*` 외 모델은 `unsupported`. spec `model` 키는 무시하고 `warnings`에 알림 | 현재 모델은 모두 Images API 또는 Gemini native (§5.1) |
| 리다이렉트 | 모델 조회·생성·편집 모두 따라가지 않고 `redirect` 오류. Python도 2026-10-18 수정(`fix: stop following redirects`)으로 같아짐 | API 키가 다른 호스트로 가지 않게 (§5.4, 검토 P1-01) |
| 기록 JSON 새 필드 | `status`(pending/complete/failed), `request_id`, `request_hash`, `background`, `fit`, `provider_size`, `saved_size`, `sha256`, `alpha`, `connection_source`, `error`, `engine`. Python 키는 모두 같은 값으로 유지 | 이미지와 기록을 하나의 완료 결과로 공개 (§5.4, P1-05) |
| 실패 기록 | 실패하면 `status:"failed"`와 오류 구조를 남김. Python은 요청 전 기록만 남김 | 갤러리가 완료/실패를 구분 (§7.5) |
| 동시 실행 | 같은 출력 경로를 다른 실행이 쓰는 중이면 API 호출 없이 `conflict` 실패 | 이중 과금·기록 뒤섞임 방지 (§5.4) |
| resume 판정 | 파일 존재만이 아니라 기록의 `request_hash`·`sha256`과 이미지 디코딩까지 확인. 다른 요청으로 만든 파일이면 건드리지 않고 `conflict`. 기록 파일이 없거나 `status`가 없는 유효한 Python 기록이면 이미지가 열릴 때 완료로 인정. 읽을 수 없거나 JSON으로 해석되지 않는 기록은 손상으로 보고 `conflict`(재사용·덮어쓰기·생성 요청 없음). 판정은 출력 lock을 잡은 뒤 다시 하므로, 다른 실행이 먼저 완료한 기록은 덮어쓰지 않음 | 다른 프롬프트의 결과를 재사용하지 않게 (§5.4) |
| 실패 사유 문구 | `failures[].reason`은 `HTTP 400: rejected`처럼 정제한 한 줄. Python은 자식 프로세스 stderr 마지막 줄. 대신 `failures[].error`에 `category`, `provider_code`, `http_status`, `retryable` | 원인 구분과 키 제거 (§5.4) |
| 최상위 오류 | `error_category`를 함께 냄 | 같은 이유 |
| 새 spec 키 | `background`, `fit` 지원. `mask`, `keep_outside_mask`, `remove_background:true`(단계 4)는 유료 호출 전에 `unsupported`로 거부. 최상위 `profile`은 지원(Python에는 없음), job 안의 `profile`은 `unsupported`(한 실행은 한 연결) | 조용히 무시하지 않음 (§5.2, P1-04) |
| 연결 프로필 | `image-cli profile list/add/update/remove/default/check`와 `--profile NAME`. 프로필(종류 `cliproxy`·`openai`·`gemini`·`xai`·`openai_compatible` + 주소 + 키 참조)은 `IMAGE_STUDIO_HOME` 또는 `%APPDATA%\ImageStudio\profiles.json`, 키는 Windows 자격 증명 관리자(`ImageStudio`, 사용자 = 프로필 id)에만 저장. 명시한 프로필이 없거나 키가 없으면 다른 출처를 빌리지 않고 `invalid_request`. Python에는 없음 | 앱과 CLI가 같은 연결 설정을 씀, 키 평문 금지 (§6.1) |
| 공식 API 어댑터 | OpenAI(`gpt-image-*`, Bearer, JSON 편집, 참조 최대 16장), Gemini(`gemini-*image*`, `x-goog-api-key`, `/v1beta/models` 페이지 처리), xAI(`grok-imagine-*`, JSON `/images/edits`, `grok-imagine-image-2.0` 참조 최대 5장, 그 밖의 grok 1장). 종류와 모델이 맞지 않으면 유료 호출 전에 `unsupported`. 공식 API 직접 호출은 문서 근거(2026-10-18)이며 미실측 | 공급자별 요청 형식 차이 (§6.3, §6.4) |
| 동시 실행 기본값 | `cliproxy`·`openai_compatible` 30(Python과 같음), `openai`·`gemini`·`xai` 4. 프로필 `concurrency` → spec `concurrency` 순서로 덮어씀 | 공식 API 요금·속도 제한에 보수적으로 (§6.1) |
| 연결 상태 기록 | 프로필로 실제 생성하면 마지막 결과(성공 또는 HTTP/네트워크 실패)를 `status.json`에 남기고 `profile check`의 `last_generation`으로 보여 줌. `profile check`는 생성하지 않음 | "실제 생성 가능"은 마지막 생성으로만 판정 (§6.2) |
| `--list-models` | 해석된 연결(프로필 포함)의 종류에 맞는 이미지 모델만 보이고 `profile`·`kind`를 함께 냄 | 프로필마다 쓸 수 있는 모델이 다름 (§6.3) |
| 크기 맞춤 | `fit:"cover"`(기본, Python과 같은 중앙 자르기) 외에 `contain`(투명 여백). 투명 배경 요청의 기본은 `contain` | 투명 소재가 잘리지 않게 (§7.1, P1-02) |
| batch 결과 | `stdout_tail`/`stderr_tail` 없음(자식 프로세스 없음). `concurrency`, `retried` 추가. 실패 20건 뒤 시작하지 않은 계획을 `cancelled` 실패로 보고(Python은 결과에서 빠짐) | 프로세스 안 실행, 정직한 결과 |
| batch 진행 이벤트 | `mode:"batch"`의 `job_*` 이벤트를 stderr에 냄. Python batch는 이벤트 없음 | 앱 진행 표시 |
| preset 상대 경로 | 현재 폴더 → 실행 파일 위쪽 폴더 중 그 경로가 있는 곳. Python은 저장소 루트 기준 | 실행 파일이 저장소 밖에 있을 수 있음 |
| 이미지 픽셀 | 크기와 형식은 같지만 리샘플링 구현(Pillow LANCZOS vs `image` Lanczos3)이 달라 픽셀은 다를 수 있음 | 비결정적 생성 이미지는 바이트 비교 대상이 아님 (§5.3) |
| `elapsed_sec`, `ts` | 비교에서 제외 | 실행 환경 값 |
