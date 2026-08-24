# Grok 이미지 및 Oh My Pi 통합 설계

## 목표

CLIProxyAPI가 이미 노출하는 Grok 모델을 두 소비자에 정확히 연결한다.

- `E:/ai-image-generator`: Grok 이미지 모델 3개로 single, jobs, batch 생성을 실행할 수 있다.
- Oh My Pi: `cliproxy-xai/grok-4.6:xhigh`를 선택하고 기본 모델로 사용할 수 있다.
- `E:/cliproxyapi/release`: 기존 정상 노출을 유지하며 중복 alias나 불필요한 설정을 추가하지 않는다.

## 확인된 현재 상태

- CLIProxyAPI `/v1/models`는 `grok-4.6`, `grok-imagine-image-2.0`, `grok-imagine-image`, `grok-imagine-image-quality`를 이미 반환한다.
- `E:/cliproxyapi/release/config.yaml`의 `disable-image-generation: "chat"`은 chat 경로의 자동 이미지 도구 주입만 막고 전용 `/v1/images/*` endpoint는 유지한다.
- `/v1/images/generations`의 `OPTIONS` 요청은 HTTP 204를 반환한다.
- `scripts/gen_image.py:image_backend`는 `grok-imagine-*`를 이미 `/v1/images/generations` adapter로 보낸다.
- 이미지 생성기에는 Grok 2.0 계약 테스트, 수동 메뉴 모델 선택, batch 진입점의 `image_model` override가 없다.
- `C:/Users/js/.omp/agent/models.yml`의 `cliproxy-xai`에는 Grok 4.3과 4.5만 등록돼 있다.
- `C:/Users/js/.omp/agent/config.yml`의 기본 selector `xai-oauth/grok-4.6:xhigh`는 사용자 모델 registry에 존재하지 않는다.

## 선택한 방향

기존 provider seam을 유지하고 소비자 registry만 명시적으로 보강한다.

- CLIProxyAPI에 같은 모델을 다시 alias하지 않는다. 현재 이름이 이미 고유하고 요청 라우팅도 성공적으로 해석된다.
- 이미지 생성기는 provider별 별도 진입점을 늘리지 않는다. 기존 `image_model` interface와 `image_backend` adapter를 사용한다.
- Oh My Pi는 새 provider를 만들지 않는다. 기존 `cliproxy-xai` provider에 Grok 4.6을 추가한다.
- 이미지 생성 기본 모델은 `gpt-image-2`로 유지한다. Grok은 명시 선택한다.

## 모델 계약

### Grok 이미지

지원 모델 ID:

- `grok-imagine-image-2.0` — 메뉴의 권장 Grok 선택
- `grok-imagine-image`
- `grok-imagine-image-quality`

공통 동작:

- endpoint: `POST /v1/images/generations`
- text-to-image만 지원
- `reference_images`가 있으면 network 요청 전에 명확한 오류 반환
- provider 응답 이미지는 기존 정규화 경로를 거쳐 사용자가 요청한 정확한 크기로 저장
- 기존 파일은 덮어쓰지 않는 상위 workflow 규칙 유지

### Grok 4.6 텍스트/멀티모달

Oh My Pi의 기존 `xai-oauth` model cache에서 확인된 계약을 `cliproxy-xai`에 옮긴다.

- selector: `cliproxy-xai/grok-4.6`
- interface: `openai-responses`
- context window: 500,000
- max tokens: 500,000
- input: text, image
- reasoning: enabled
- effort levels: `minimal`, `low`, `medium`, `high`, `xhigh`
- default selector: `cliproxy-xai/grok-4.6:xhigh`

## 변경 대상

### `E:/ai-image-generator`

- `tests/test_generator_observability.py`
  - 세 Grok 이미지 ID의 backend 선택, request payload, endpoint를 검증한다.
  - `tools/ai_image.py`가 batch에서도 `image_model`을 전달하는 계약을 검증한다.
- `scripts/gen_image.py`
  - 기존 Grok adapter를 유지한다. 테스트가 드러내는 실제 계약 차이만 수정한다.
- `scripts/gen_batch.py`
  - preset 값을 덮어쓸 수 있는 `--image-model` 선택을 추가한다.
- `tools/ai_image.py`
  - mode=batch의 spec-level `image_model`을 `gen_batch.py`에 전달한다.
  - schema/example에 Grok 2.0을 명시한다.
- `tools/Image-Menu.ps1`
  - OpenAI, Gemini, Grok 이미지 모델을 선택하고 single과 batch 명령에 전달한다.
- `README.md`
  - 세 Grok 모델, text-to-image 제한, JSON과 메뉴 사용법을 기록한다.

### `C:/Users/js/.omp/agent`

- `models.yml`
  - equivalence override `cliproxy-xai/grok-4.6: grok-4.6`을 추가한다.
  - 기존 `cliproxy-xai` provider의 models 목록에 Grok 4.6 metadata를 추가한다.
- `config.yml`
  - 잘못된 `xai-oauth/grok-4.6:xhigh`를 `cliproxy-xai/grok-4.6:xhigh`로 교체한다.

### `E:/cliproxyapi/release`

파일 변경 없음. 다음 상태만 검증한다.

- `grok-4.6`과 Grok 이미지 3개가 `/v1/models`에 존재
- `/v1/images/generations`가 활성
- xAI OAuth 제외 규칙이 이 네 모델을 차단하지 않음

## 오류 처리

- Grok text model을 `image_model`로 선택하지 않는다. 메뉴에는 `grok-4.6`을 이미지 선택지로 노출하지 않는다.
- Grok 이미지 reference editing은 현재 지원하지 않으며 기존 오류를 유지한다.
- model catalog에 없는 OMP selector를 fallback으로 숨기지 않는다. registry와 default selector를 함께 바꾼다.
- CLIProxyAPI의 현재 정상 노출을 중복 alias로 덮지 않는다.

## 검증

1. 기존 Python 테스트가 변경 전 통과하는 상태를 기준선으로 유지한다.
2. 새 Grok 계약 테스트를 먼저 실패시킨 뒤 최소 구현으로 통과시킨다.
3. PowerShell 메뉴가 선택한 `--image-model`을 single과 batch 명령에 포함하는지 확인한다.
4. YAML을 파싱하고 `omp models find grok --json`에서 `cliproxy-xai/grok-4.6`을 확인한다.
5. `omp`가 `cliproxy-xai/grok-4.6:xhigh` selector를 해석하는지 확인한다.
6. CLIProxyAPI `/v1/models`에서 텍스트 1개와 이미지 3개를 확인한다.
7. 사용자 승인 직후 Grok 4.6 짧은 text 요청 1회와 `grok-imagine-image-2.0` low-quality 이미지 1회를 실행한다. 이미지 출력은 새 `ImageGallery/grok-integration-smoke/` package에 저장하고 기존 파일을 덮어쓰지 않는다.

## 비범위

- Grok video 모델
- Grok reference-image editing
- CLIProxyAPI alias 재설계
- 다른 OMP role을 Grok 4.6으로 변경
- 기존 Grok 4.3/4.5 등록 제거
