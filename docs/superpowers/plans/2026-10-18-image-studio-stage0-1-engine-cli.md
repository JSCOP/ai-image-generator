# Image Studio 단계 0–1: Python 보안 수정 + Rust 엔진·CLI 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 기존 Python CLI의 리다이렉트 키 전달 문제를 고치고, `tools/ai_image.py`와 같은 JSON 계약을 지키는 Rust 엔진(`image-core`)과 CLI(`image-cli`)를 만든다. 리다이렉트 차단, 파일 예약·완료·resume 검증, 맞춤 방식(cover/contain), 새 키의 사전 검증은 처음부터 엔진 규칙으로 넣는다.

**Architecture:** Cargo workspace에 `crates/image-core`(라이브러리, 모든 동작), `crates/image-cli`(얇은 실행 파일), `crates/test-fixture`(테스트 전용 가짜 제공자)를 둔다. 생성은 자식 프로세스 없이 프로세스 안에서 tokio 작업으로 병렬 실행한다. Python 코드는 이 단계에서 지우지 않는다(이전 기간 동안 에이전트가 계속 사용).

**Tech Stack:** Rust 1.97 (edition 2021), tokio 1, reqwest 0.12 (`rustls-tls`, `json`, 기본 기능 끔), image 0.25, serde/serde_json, sha2, base64, uuid, rand, regex, thiserror, tokio-util(CancellationToken), winreg(Windows), windows-sys(Windows, 프로세스 생존 확인); 테스트 전용 tiny_http, tempfile. Python 쪽은 표준 라이브러리 unittest.

**Spec:** `docs/superpowers/specs/2026-10-18-image-studio-rust-app-design.md` (§5, §5.4, §6.4, §7.1, §10 단계 0–1). 외부 검토: `docs/superpowers/reviews/2026-10-18-image-studio-rust-app-review.md`.

이후 단계(2 프로필, 3 앱, 4 마스크·배경 제거, 5 설치 파일, 6 Python 정리)는 이 계획이 끝난 뒤 각각 별도 계획서로 쓴다.

## Global Constraints

- 커밋 전 `C:/Users/js/AppData/Local/Programs/Python/Python313/python.exe C:/Users/js/.config/git-identity-guard/guard.py check --repo E:/ai-image-generator`를 실행한다. 작성자는 JSCOP. `--no-verify` 금지.
- API 키는 저장소·spec·로그·기록 JSON·오류 메시지 어디에도 평문으로 남기지 않는다. 오류 문자열은 키 값과 `Bearer <값>`을 `***`로 바꾼 뒤 내보낸다.
- 기존 이미지는 덮어쓰지 않는다. 최종 파일은 create-new 또는 같은 폴더 rename으로만 공개한다.
- 요청한 모델을 다른 모델로 바꾸지 않는다.
- Rust에는 머신별 하드코딩 주소(`DESKTOP-SB818KQ`, `100.98.54.122`)를 넣지 않는다. 연결 주소가 없으면 네트워크 호출 전에 오류를 낸다.
- Responses API 경로(`POST /v1/responses`)는 옮기지 않는다. spec의 `model` 키는 받아서 무시하고 결과 `warnings`에 알린다.
- 리다이렉트는 따라가지 않는다(`reqwest::redirect::Policy::none()`). 3xx는 `category:"redirect"` 오류다.
- 기본값은 Python과 같다: size `1920x1080`, quality `high`, image_model `gpt-image-2.5`(환경변수 `CLIPROXY_IMAGE_MODEL`이 있으면 그 값), concurrency 30(1..32로 제한), retries 2(0..5), job_timeout_sec `CLIPROXY_TIMEOUT` 또는 600(<=0이면 끔), 재시도 백오프 기준 `AI_IMAGE_RETRY_BACKOFF_SEC` 또는 5.
- 출력 형식은 항상 PNG이다(Python CLI도 PNG만 요청한다).
- 실제 제공자 호출은 Task 12의 2장뿐이다. 나머지 테스트는 모두 로컬 fixture를 쓴다.
- 오류 분류 값은 spec §5.4와 같다: `network, dns, timeout, tls, auth, permission, quota, rate_limit, conflict, cancelled, not_found, redirect, unsupported, invalid_request, provider, internal`.

## Review Focus

1. 제공자가 HTTP 200이면서 이미지가 아닌 본문(HTML, 잘린 base64, 빈 `data`)을 주는 경우: 이미지 파일을 만들지 않고, 기록은 `status:"failed"`, `category:"provider"`, `retryable:true`(Python의 EXIT_TRANSIENT와 같다). 테스트는 Task 7에 둔다.
2. 경로 문자가 섞인 topic이나 job id(`a/b`, `..`, `C:\x`, 긴 한국어): 결과 파일은 항상 `output/<slug>/` 안에 생긴다. Task 2의 slug 테스트와 Task 8의 jobs 테스트가 확인한다.
3. 프록시가 꺼진 상태에서 30장을 요청하는 경우: 장마다 `network` 오류로 재시도를 다 쓴 뒤 실패하고, 이미지와 lock이 남지 않으며, 결과는 `ok:false`이고 실패 사유가 구조화되어 있다. 테스트는 Task 8.
4. 실행 중 취소(Ctrl+C): 진행 중이던 장은 `.part`가 최종 파일로 공개되지 않고, lock이 지워지며, 기록에 `cancelled`가 남는다. 그 뒤 resume하면 그 장을 다시 만든다. 테스트는 Task 8.
5. 존재하지 않거나 이미지가 아닌 참조 파일, 상대 경로 참조: 유료 호출 전에 실패한다. 상대 경로는 Python처럼 현재 작업 폴더 기준으로 해석한다. 테스트는 Task 7.

---

## File Structure

```
Cargo.toml                         # [workspace] members, 공용 의존성 버전
rust-toolchain.toml                # channel = "1.97"
.gitignore                         # /target 추가
crates/image-core/
  Cargo.toml
  src/lib.rs                       # 모듈 공개
  src/error.rs                     # EngineError, Category, redact()
  src/catalog.rs                   # 모델 표, Backend 판정, 기능(참조/투명)
  src/size.rs                      # Size, parse/패딩/Gemini 버킷
  src/paths.rs                     # safe_topic_slug, gallery_root, topic_dirs, metadata_path
  src/connection.rs                # EnvSource, Secret, normalize_base_url, resolve_connection
  src/http.rs                      # Http(리다이렉트 차단), post_json, get
  src/provider.rs                  # ImageRequest, 요청 본문, 응답 해석, HTTP 오류 분류, list_models
  src/normalize.rs                 # Fit, normalize(), alpha_stats()
  src/store.rs                     # Record, request_hash, reserve, inspect_existing, publish
  src/generate.rs                  # generate_one: 한 장 생성의 전체 흐름
  src/runner.rs                    # spec 해석, single/jobs/batch, 병렬·재시도·타임아웃·취소·진행 이벤트
  src/batch.rs                     # preset 로딩, Python 호환 난수(PyRandom), 계획 생성
  src/schema.rs                    # --schema, --example JSON
crates/image-core/tests/*.rs       # 모듈별 통합 테스트
crates/test-fixture/               # publish=false, 가짜 제공자 서버(tiny_http)
  src/lib.rs
crates/image-cli/
  src/main.rs                      # 인자, stdin/--spec/--json, 종료 코드, Ctrl+C
  tests/cli.rs                     # 실행 파일 계약 테스트
  tests/parity.rs                  # Python CLI와 비교(.venv 없으면 건너뜀)
scripts/gen_image.py               # Task 0: 리다이렉트 차단
tools/image_core.py                # Task 0: 공용 NoRedirect 사용
tests/test_redirect_security.py    # Task 0
docs/RUST_PARITY.md                # 의도된 차이 목록
```

## 공용 인터페이스 (여러 Task가 같이 쓰는 이름)

```rust
// error.rs
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)] #[serde(rename_all = "snake_case")]
pub enum Category { Network, Dns, Timeout, Tls, Auth, Permission, Quota, RateLimit, Conflict,
                    Cancelled, NotFound, Redirect, Unsupported, InvalidRequest, Provider, Internal }
#[derive(Serialize, Clone, Debug, thiserror::Error)] #[error("{message}")]
pub struct EngineError { pub category: Category, pub provider_code: Option<String>,
                         pub http_status: Option<u16>, pub retryable: bool, pub message: String }
impl EngineError { pub fn new(category: Category, message: impl Into<String>) -> Self; // retryable = category의 기본값
                   pub fn reason(&self) -> String; } // 결과 JSON의 "reason": 300자 이하 한 줄
pub fn redact(text: &str, secret: Option<&Secret>) -> String;

// catalog.rs
pub enum Backend { Gemini, OpenAiImages, Unsupported }
pub fn backend(model: &str) -> Backend;
pub fn is_listable_image_model(id: &str) -> bool;            // ai_image.list_models의 필터
pub fn supports_references(model: &str) -> bool;             // gemini, gpt-image-*
pub fn supports_native_transparency(model: &str) -> bool;    // gpt-image-2.5-sunburst, gpt-image-2.5-flare
pub fn catalog() -> &'static [ModelInfo];                     // tools/image_core.py MODELS의 9개

// size.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)] pub struct Size { pub width: u32, pub height: u32 }
pub fn parse_size(text: &str) -> Result<Size, String>;       // 오류 문구는 ai_image._validate_size와 같음
impl Size { pub fn padded16(self) -> Size; pub fn to_string(&self) -> String; } // "WxH"
pub fn gemini_aspect_ratio(size: Size) -> &'static str;
pub fn gemini_image_size(size: Size) -> &'static str;

// connection.rs
pub struct Secret(String);                                   // Debug/Display는 "***"
pub trait EnvSource: Send + Sync { fn process(&self, key: &str) -> Option<String>;
                                   fn user(&self, key: &str) -> Option<String>; }
pub struct SystemEnv;                                        // 프로세스 env + HKCU\Environment
pub struct Connection { pub base_url: String, pub api_key: Option<Secret>, pub source: &'static str }
pub fn normalize_base_url(raw: &str) -> Result<String, String>;
pub fn resolve_connection(env: &dyn EnvSource) -> Result<Connection, EngineError>;

// provider.rs
pub enum Action { Auto, Generate, Edit }
pub enum Background { Auto, Opaque, Transparent }
pub struct ImageRequest { pub prompt: String, pub image_model: String, pub size: Size, pub quality: String,
                          pub action: Action, pub references: Vec<PathBuf>, pub background: Background }

// normalize.rs
pub enum Fit { Cover, Contain }

// runner.rs
pub type Progress = Arc<dyn Fn(serde_json::Value) + Send + Sync>;
pub struct RunOutcome { pub result: serde_json::Value, pub exit_code: i32 }
pub async fn run_spec(spec: serde_json::Value, env: Arc<dyn EnvSource>, progress: Progress,
                      cancel: CancellationToken) -> RunOutcome;
```

---

### Task 0: Python 생성 경로의 리다이렉트 차단 (검토 P1-01)

**Files:**
- Modify: `scripts/gen_image.py:348-366`(`_call_json_blocking`), `:513-535`(`_call_cliproxy_blocking`), `:643-652`(오류 출력)
- Modify: `tools/image_core.py:146-150`(자체 `_NoRedirect` 삭제, gen_image의 것을 사용)
- Test: `tests/test_redirect_security.py`

**Interfaces:**
- Produces: `gen_image.NoRedirect(urllib.request.HTTPRedirectHandler)`, `gen_image.open_no_redirect(request, timeout)`. image_core는 `gen_image.open_no_redirect`로 연결 확인을 한다.

- [ ] **Step 1: 실패하는 테스트 작성**

`tests/test_redirect_security.py`: `ThreadingHTTPServer` 두 개를 띄운다. A는 모든 요청에 `302 Location: http://localhost:<B포트><같은 경로>`를 주고, B는 받은 요청의 `Authorization` 헤더를 리스트에 저장한 뒤 이미지 응답을 준다. `GenerateImageOptions(api_key="fake-token", base_url="http://127.0.0.1:<A>/v1", ...)`로 다음을 확인한다.

```python
def test_generation_does_not_follow_redirects(self):
    body, status, _url = gen_image.call_native_image_backend(self.options("gpt-image-2.5"))
    self.assertEqual(status, 302)
    self.assertEqual(self.destination_auth, [])

def test_gemini_generation_does_not_follow_redirects(self):   # gemini-3.1-flash-image
def test_edit_does_not_follow_redirects(self):                 # 참조 1장, action="edit"
def test_model_check_does_not_follow_redirects(self):
    result = image_core.check_connection("http://127.0.0.1:<A>/v1", "fake-token")
    self.assertFalse(result["ok"]); self.assertEqual(self.destination_auth, [])
def test_redirect_status_is_not_transient(self):
    # main()을 패치된 argv로 실행해 반환값이 1이고 stderr에 "redirect"가 들어 있음
```

- [ ] **Step 2: 실패 확인**

Run: `.venv/Scripts/python.exe -m unittest tests.test_redirect_security -v`
Expected: 생성·편집·Gemini 테스트가 `destination_auth == ['Bearer fake-token']`이라 FAIL. 모델 확인 테스트는 PASS.

- [ ] **Step 3: 구현**

`gen_image.py`에 `NoRedirect`(redirect_request가 None 반환)와 `open_no_redirect(request, timeout)`(`urllib.request.build_opener(NoRedirect).open(...)`)를 두고, 두 `urlopen` 호출을 바꾼다. 3xx는 `HTTPError`로 들어오므로 `(본문, 코드)`로 반환된다. `main()`의 `status != 200` 분기에서 300–399이면 `"CLIProxyAPI redirected to another address (HTTP {status}); not following it so the API key stays on the configured host. Check CLIPROXY_BASE_URL."` 한 줄만 출력하고 본문은 출력하지 않은 채 1을 반환한다. `image_core.py`는 `gen_image.open_no_redirect`를 쓰고 자체 클래스를 지운다.

- [ ] **Step 4: 통과 확인**

Run: `.venv/Scripts/python.exe -m unittest discover -s tests -v`
Expected: 기존 34개 + 신규 5개, 모두 OK.

- [ ] **Step 5: 커밋** — 가드 점검 후 `fix: stop following redirects on image generation requests`.

---

### Task 1: 단계 0 연결 기준선 (이 PC)

코드 변경 없음. 노트북 확인은 노트북이 켜져 있을 때 사용자와 함께 한다. 이 단계는 Rust 작업을 막지 않는다.

- [ ] **Step 1:** 이 PC의 Windows 사용자 환경변수 `CLIPROXY_BASE_URL`이 비어 있는지 확인한 뒤 `http://127.0.0.1:8317/v1`로 설정한다(`[Environment]::SetEnvironmentVariable('CLIPROXY_BASE_URL','http://127.0.0.1:8317/v1','User')`). 이 값은 Python에서도 하드코딩 규칙보다 먼저 적용된다.
- [ ] **Step 2:** 새 셸에서 `.venv/Scripts/python.exe tools/ai_image.py --list-models`를 실행한다. Expected: `ok:true`, `base_url`이 `http://127.0.0.1:8317/v1`. 프록시가 꺼져 있으면 사용자에게 실행을 요청한다.
- [ ] **Step 3:** `tailscale status`와 `tailscale ip -4`로 이 PC의 현재 Tailscale 상태와 주소를 기록한다. 과거 주소(`100.98.54.122`)와 다르면 문서에는 현재 값을 쓴다.
- [ ] **Step 4 (사용자와 함께, 노트북이 켜져 있을 때):** 노트북에서 `CLIPROXY_BASE_URL=http://<현재 Tailscale IP>:8317/v1`로 `--list-models`를 실행하고 1장을 생성한다. 결과(성공 또는 막힌 지점)를 `docs/SSOT.md` 머신 토폴로지 절에 날짜와 함께 적는다.

---

### Task 2: Workspace 뼈대 + 카탈로그·크기·경로 순수 함수

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `crates/image-core/Cargo.toml`, `src/lib.rs`, `src/catalog.rs`, `src/size.rs`, `src/paths.rs`
- Modify: `.gitignore`(`/target/`)
- Test: `crates/image-core/tests/pure.rs`

**Interfaces:** Produces `catalog.rs`, `size.rs`, `paths.rs`의 공용 인터페이스(위 표). `paths`:
`safe_topic_slug(&str) -> String`, `gallery_root(&Path) -> PathBuf`, `topic_dirs(&Path, &str) -> (PathBuf, PathBuf)`, `metadata_path(output: &Path, workspace: &Path, slug: &str) -> PathBuf`.

- [ ] **Step 1: 실패하는 테스트 작성** (`tests/pure.rs`)

```rust
#[test] fn slug_matches_python() {
    assert_eq!(safe_topic_slug("  my topic/v2! "), "my_topic_v2");
    assert_eq!(safe_topic_slug("공항 보안 검색대"), "공항_보안_검색대");
    assert_eq!(safe_topic_slug("../.."), "image-request");
    assert_eq!(safe_topic_slug(r"C:\x"), "C__x");
    assert_eq!(safe_topic_slug(&"가".repeat(100)).chars().count(), 80);
}
#[test] fn sizes() {
    assert_eq!(parse_size("1920x1080").unwrap(), Size { width: 1920, height: 1080 });
    assert_eq!(parse_size("1920X1080").unwrap().width, 1920);
    assert_eq!(parse_size("0x5").unwrap_err(), "size 0x5: width and height must be positive");
    assert_eq!(parse_size("big").unwrap_err(), "invalid size 'big'; expected WIDTHxHEIGHT");
    assert_eq!(parse_size("1000x1000").unwrap().padded16().to_string(), "1008x1008");
    assert_eq!(gemini_aspect_ratio(parse_size("1920x1080").unwrap()), "16:9");
    assert_eq!(gemini_image_size(parse_size("768x768").unwrap()), "512");
    assert_eq!(gemini_image_size(parse_size("1536x1024").unwrap()), "1K");
    assert_eq!(gemini_image_size(parse_size("3072x10").unwrap()), "2K");
    assert_eq!(gemini_image_size(parse_size("3840x2160").unwrap()), "4K");
}
#[test] fn backends_and_capabilities() {
    assert!(matches!(backend("gemini-3.1-flash-image"), Backend::Gemini));
    assert!(matches!(backend("grok-imagine-image-2.0"), Backend::OpenAiImages));
    assert!(matches!(backend("gpt-5.5"), Backend::Unsupported));
    assert!(!supports_references("grok-imagine-image"));
    assert!(supports_native_transparency("gpt-image-2.5-sunburst"));
    assert!(!supports_native_transparency("gpt-image-2.5"));
    assert_eq!(catalog().len(), 9);
}
#[test] fn gallery_paths() {
    let ws = tempfile::tempdir().unwrap();
    let (out, meta) = topic_dirs(ws.path(), "t");
    assert_eq!(out, ws.path().canonicalize().unwrap().join("ImageGallery/output/t"));
    let nested = ws.path().join("ImageGallery/output");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(gallery_root(&nested), ws.path().canonicalize().unwrap().join("ImageGallery"));
    assert_eq!(metadata_path(&out.join("a.png"), ws.path(), "t"), meta.join("a.png.json"));
}
```

`safe_topic_slug`는 Python 정규식 `[^\w\s.-]+ → "_"`, `\s+ → "_"`, 양끝 `._- ` 제거, 80자(문자 수), 비면 `"image-request"`와 같다. Rust `regex`의 `\w`·`\s`는 기본이 유니코드라 같은 결과다. `gallery_root`는 경로를 canonicalize한 뒤(없으면 절대 경로화) 조상 중 이름이 대소문자 무시로 `imagegallery`인 것을 고른다. Windows의 `\\?\` 접두어는 `dunce`식으로 떼어 낸다(결과 JSON 경로가 Python과 같은 모양이어야 함).

- [ ] **Step 2:** `cargo test -p image-core --test pure` → 컴파일 오류(FAIL).
- [ ] **Step 3:** 위 시그니처대로 구현한다. 카탈로그 9개의 id·label·provider·reference_images는 `tools/image_core.py:64-81`을 그대로 옮긴다. `is_listable_image_model`은 `backend == Gemini || id.starts_with("gpt-image-") || id.starts_with("grok-imagine-image")`(소문자 비교).
- [ ] **Step 4:** `cargo test -p image-core --test pure` → PASS. `cargo clippy --workspace -- -D warnings` → 경고 없음.
- [ ] **Step 5: 커밋** `feat(core): add Rust workspace with catalog, size and gallery path rules`.

---

### Task 3: 오류 구조·연결 해석·리다이렉트 차단 HTTP + 가짜 제공자

**Files:**
- Create: `crates/image-core/src/error.rs`, `src/connection.rs`, `src/http.rs`, `crates/test-fixture/{Cargo.toml,src/lib.rs}`
- Test: `crates/image-core/tests/http.rs`

**Interfaces:**
- Produces: `error.rs`, `connection.rs`(위 표). `http.rs`:
  `pub struct Http`; `Http::new(timeout: Option<Duration>) -> Http`;
  `async fn post_json(&self, url: &str, body: &Value, key: Option<&Secret>) -> Result<(u16, Vec<u8>), EngineError>`;
  `async fn get(&self, url: &str, key: Option<&Secret>, extra: &[(&str, String)]) -> Result<(u16, Vec<u8>), EngineError>`.
  3xx는 `Err(category Redirect, http_status Some(code), retryable false)`. 그 밖의 HTTP 상태는 `Ok`로 돌려주고 분류는 Task 4가 한다.
- Produces (test-fixture): `FakeProvider::start(handler: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> FakeProvider`, `.base_url() -> String`(`http://127.0.0.1:<port>/v1`), `.requests() -> Vec<Recorded>`. `Recorded { method, path, headers: Vec<(String,String)>, body: Vec<u8> }`, `Reply { status: u16, headers: Vec<(String,String)>, body: Vec<u8>, delay: Duration }`. 도우미 `png_bytes(w, h, rgba: [u8;4]) -> Vec<u8>`, `openai_image_reply(png) -> Reply`, `gemini_image_reply(png, model) -> Reply`.

- [ ] **Step 1: 실패하는 테스트 작성** (`tests/http.rs`)

```rust
#[tokio::test] async fn redirect_is_not_followed_and_key_stays_home() {
    let dest = FakeProvider::start(|_| Reply::json(200, "{}"));
    let target = format!("{}/images/generations", dest.base_url());
    let src = FakeProvider::start(move |_| Reply::redirect(302, &target));
    let http = Http::new(Some(Duration::from_secs(5)));
    let key = Secret::new("fake-token");
    let err = http.post_json(&format!("{}/images/generations", src.base_url()), &json!({}), Some(&key)).await.unwrap_err();
    assert_eq!(err.category, Category::Redirect);
    assert_eq!(err.http_status, Some(302));
    assert!(dest.requests().is_empty());
    let err = http.get(&format!("{}/models", src.base_url()), Some(&key), &[]).await.unwrap_err();
    assert_eq!(err.category, Category::Redirect);
}
#[tokio::test] async fn connection_refused_is_retryable_network() { /* 닫힌 포트 → Network, retryable */ }
#[tokio::test] async fn slow_server_times_out() { /* delay 2s, timeout 200ms → Timeout, retryable */ }
#[test] fn base_url_rules() {
    assert_eq!(normalize_base_url("http://127.0.0.1:8317").unwrap(), "http://127.0.0.1:8317/v1");
    assert_eq!(normalize_base_url(" http://host:1/v1/ ").unwrap(), "http://host:1/v1");
    assert!(normalize_base_url("http://u:p@host/v1").is_err());
    assert!(normalize_base_url("ftp://host").is_err());
    assert!(normalize_base_url("http://host/v1?x=1").is_err());
}
#[test] fn connection_resolution_order_and_no_default() {
    let env = MapEnv::new(&[], &[]);                         // 테스트용 EnvSource
    let err = resolve_connection(&env).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("CLIPROXY_BASE_URL"));
    let env = MapEnv::new(&[("CLIPROXY_BASE_URL", "http://a:1")], &[("CLIPROXY_BASE_URL", "http://b:2"), ("CLIPROXY_API_KEY", "k")]);
    let c = resolve_connection(&env).unwrap();
    assert_eq!(c.base_url, "http://a:1/v1");
    assert_eq!(c.source, "process_env");
    assert_eq!(format!("{:?}", c.api_key), "Some(***)");
}
#[test] fn redact_removes_key_and_bearer() {
    let s = Secret::new("sk-123");
    assert_eq!(redact("bad sk-123 Authorization: Bearer sk-999", Some(&s)), "bad *** Authorization: Bearer ***");
}
```

키 해석 순서는 Python `resolve_api_key`와 같다: 프로세스 `CLIPROXY_API_KEY` → 프로세스 `OPENAI_API_KEY` → 사용자 `CLIPROXY_API_KEY` → 사용자 `OPENAI_API_KEY`. 주소 해석은 프로세스 → 사용자 `CLIPROXY_BASE_URL` 순서이고, 둘 다 없으면 오류 `"연결 설정이 없습니다. CLIPROXY_BASE_URL(예: http://127.0.0.1:8317/v1)과 CLIPROXY_API_KEY를 설정해 주세요."`(InvalidRequest)를 낸다. `source`는 `"process_env"` 또는 `"user_env"`이며 주소의 출처를 따른다. 키가 없어도 연결은 만들고(Python 동작), 요청은 인증 헤더 없이 보낸다.

- [ ] **Step 2:** `cargo test -p image-core --test http` → FAIL.
- [ ] **Step 3:** 구현. 재시도 기본값: network·dns·timeout·rate_limit·provider는 retryable true, 나머지는 false. reqwest 오류는 `is_timeout()`이면 Timeout, `is_connect()`이면 원인 문자열에 dns/resolve가 있을 때 Dns, 그 밖에는 Network, TLS 관련은 Tls로 분류한다. 응답 본문은 최대 64MiB까지 읽는다. `MapEnv`는 test-fixture에 둔다.
- [ ] **Step 4:** `cargo test -p image-core` → PASS.
- [ ] **Step 5: 커밋** `feat(core): add no-redirect HTTP client, structured errors and env connection`.

---

### Task 4: 제공자 요청·응답·오류 분류·모델 목록

**Files:**
- Create: `crates/image-core/src/provider.rs`
- Test: `crates/image-core/tests/provider.rs`

**Interfaces:**
- Consumes: Task 2·3.
- Produces:
  `pub struct ProviderCall { pub url: String, pub body: Value, pub backend: Backend }`;
  `pub fn build_call(base_url: &str, req: &ImageRequest) -> Result<ProviderCall, EngineError>`(참조 파일을 읽어 data URL/inlineData로 넣음);
  `pub fn extract_image(backend: &Backend, body: &[u8]) -> Result<(Vec<u8>, Option<String>), EngineError>`(이미지 바이트, response_model);
  `pub fn classify_http_error(status: u16, body: &[u8], key: Option<&Secret>) -> EngineError`;
  `pub async fn list_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError>`.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test] fn openai_generation_payload_matches_python() {
    let call = build_call("http://h/v1", &req("gpt-image-2.5", "1000x1000", vec![])).unwrap();
    assert_eq!(call.url, "http://h/v1/images/generations");
    assert_eq!(call.body, json!({"model":"gpt-image-2.5","prompt":"p","n":1,"size":"1008x1008","quality":"high","output_format":"png"}));
}
#[test] fn gpt_edit_uses_images_data_urls() { /* 참조 1장 → url .../images/edits, body.images[0].image_url가 "data:image/png;base64," 로 시작 */ }
#[test] fn gpt_edit_without_reference_is_rejected() { /* action Edit, refs 없음 → InvalidRequest "GPT image edits require at least one reference image" */ }
#[test] fn grok_payload_uses_b64_json_and_rejects_references() {
    // body에 "response_format":"b64_json", "output_format" 없음; 참조가 있으면 Unsupported
}
#[test] fn transparent_background_is_sent_only_when_requested() {
    // Background::Transparent → body["background"]=="transparent"; Opaque → "opaque"; Auto → 키 없음
    // gemini/grok에는 background를 넣지 않음
}
#[test] fn gemini_payload_and_url() {
    let call = build_call("http://h/v1", &req("gemini-3.1-flash-image", "1920x1080", vec![png_ref()])).unwrap();
    assert_eq!(call.url, "http://h/v1beta/models/gemini-3.1-flash-image:generateContent");
    assert_eq!(call.body["generationConfig"]["imageConfig"], json!({"aspectRatio":"16:9","imageSize":"2K"}));
    assert_eq!(call.body["contents"][0]["parts"][1]["inlineData"]["mimeType"], "image/png");
}
#[test] fn extracts_both_shapes() { /* openai data[0].b64_json + "model"; gemini candidates[].content.parts[].inlineData(or inline_data) + "modelVersion" */ }
#[test] fn non_image_200_is_retryable_provider_error() {
    // "<html>" 본문, {"data":[]}, 잘린 base64 → Category::Provider, retryable true
}
#[test] fn classifies_http_errors() {
    assert_eq!(classify_http_error(401, b"{}", None).category, Category::Auth);
    let e = classify_http_error(403, br#"{"error":{"code":"personal-team-blocked:spending-limit","message":"limit"}}"#, None);
    assert_eq!((e.category, e.provider_code.as_deref(), e.retryable), (Category::Quota, Some("personal-team-blocked:spending-limit"), false));
    assert_eq!(classify_http_error(403, b"{}", None).category, Category::Permission);
    assert_eq!(classify_http_error(429, b"{}", None).category, Category::RateLimit);
    assert!(classify_http_error(429, b"{}", None).retryable);
    assert_eq!(classify_http_error(429, br#"{"error":{"code":"insufficient_quota"}}"#, None).category, Category::Quota);
    assert_eq!(classify_http_error(404, b"", None).category, Category::NotFound);
    assert!(classify_http_error(503, b"", None).retryable);
    assert_eq!(classify_http_error(400, b"", None).category, Category::InvalidRequest);
}
#[tokio::test] async fn list_models_reads_data_ids_with_bearer() { /* FakeProvider GET /v1/models → ids; Authorization 헤더 확인 */ }
```

분류 규칙: `provider_code`는 JSON `error.code`(문자열) → `error.status` → `error.type` 순서로 찾는다. 코드나 `error.message`에 `spending-limit`, `quota`, `billing`이 들어 있으면(대소문자 무시) 상태와 상관없이 Quota(retryable false)다. 그 밖의 매핑은 401 Auth, 402 Quota, 403 Permission, 404 NotFound, 408·425 Timeout, 429 RateLimit, 5xx Provider(retryable), 그 밖의 4xx InvalidRequest이다. message는 `"HTTP {status} {provider_code}: {error.message 또는 본문 앞 200자}"`이고 `redact`를 거친다. 재시도 가능 상태 집합은 Python과 같다(408/425/429/500/502/503/504). 501 등 다른 5xx는 Provider지만 retryable false다.

- [ ] **Step 2:** `cargo test -p image-core --test provider` → FAIL.
- [ ] **Step 3:** 구현. MIME은 확장자로 정한다(png/jpg/jpeg/webp/gif, 모르면 image/png). Gemini 루트는 `/v1`을 떼어 만든다. `list_models`는 `GET {base}/models`를 보내고, 200이 아니면 `classify_http_error`, JSON이 아니면 Provider 오류, `data[].id`는 최대 1000개다.
- [ ] **Step 4:** PASS 확인.
- [ ] **Step 5: 커밋** `feat(core): port provider payloads, response extraction and error classification`.

---

### Task 5: 크기 맞춤(cover/contain)과 알파 통계

**Files:**
- Create: `crates/image-core/src/normalize.rs`
- Test: `crates/image-core/tests/normalize.rs`

**Interfaces:**
- Produces:
  `pub struct Normalized { pub png: Vec<u8>, pub provider_size: Size, pub saved_size: Size, pub fit_applied: &'static str }`(`"none"|"cover"|"contain"`);
  `pub fn normalize(bytes: &[u8], target: Size, fit: Fit) -> Result<Normalized, EngineError>`;
  `#[derive(Serialize)] pub struct AlphaStats { pub has_alpha_channel: bool, pub transparent_ratio: f64, pub partial_ratio: f64, pub opaque_ratio: f64, pub corner_alpha: [u8; 4] }`;
  `pub fn alpha_stats(png: &[u8]) -> Result<AlphaStats, EngineError>`.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test] fn passthrough_when_png_and_size_match() { /* 입력 바이트 == 출력 바이트, fit_applied "none" */ }
#[test] fn cover_crops_center_to_exact_size() { /* 1536x1024 → 1920x1080: saved 1920x1080, provider 1536x1024 */ }
#[test] fn jpeg_input_is_reencoded_png() { /* 같은 크기의 JPEG → PNG 시그니처, fit_applied "cover" */ }
#[test] fn contain_keeps_subject_with_transparent_padding() {
    // 1374x1145 투명 바탕, 오른쪽 끝 열(x=1373)에 불투명 빨간 세로 막대 → 1024x1024 contain
    let n = normalize(&src, Size{width:1024,height:1024}, Fit::Contain).unwrap();
    let img = image::load_from_memory(&n.png).unwrap().to_rgba8();
    assert!((1018..1024).any(|x| img.get_pixel(x, 512)[3] > 0));   // 막대가 잘리지 않음
    assert_eq!(img.get_pixel(512, 0)[3], 0);                        // 위 여백은 투명
}
#[test] fn cover_loses_edge_subject_which_is_why_contain_exists() { /* 같은 입력을 Cover로 → 오른쪽 6열 알파 모두 0 */ }
#[test] fn alpha_stats_separate_full_partial_opaque() {
    // 2x2: α=0, 0, 253, 255 → transparent .5, partial .25, opaque .25, corners [0,0,253,255]
}
#[test] fn undecodable_bytes_are_provider_error() { /* b"not an image" → Provider */ }
```

cover는 Pillow `ImageOps.fit`처럼 목표 비율에 맞춰 가운데를 자른 뒤 `Lanczos3`로 줄인다. contain은 `min(tw/w, th/h)` 배율로 줄이고 투명 RGBA 바탕 가운데에 놓는다. 입력에 알파가 없어도 contain 결과는 RGBA이다. alpha_stats의 모서리 순서는 좌상, 우상, 좌하, 우하이고, 알파 채널이 없으면 `has_alpha_channel:false`, 불투명 비율은 1.0이다.

- [ ] **Step 2:** FAIL 확인. **Step 3:** 구현. **Step 4:** `cargo test -p image-core --test normalize` PASS.
- [ ] **Step 5: 커밋** `feat(core): add cover/contain fitting and alpha statistics`.

---

### Task 6: 저장소 — 예약, 기록, 완료 공개, resume 판정 (검토 P1-05)

**Files:**
- Create: `crates/image-core/src/store.rs`
- Test: `crates/image-core/tests/store.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Serialize, Deserialize, Clone)] pub struct Record {
      pub prompt: String, pub image_model: String, pub size: String, pub quality: String, pub action: String,
      pub reference_images: Vec<String>, pub output: String,
      #[serde(skip_serializing_if = "Option::is_none")] pub response_model: Option<String>,
      // 새 필드 (Python 기록에는 없음 → 모두 Option/default)
      pub status: Option<String>, pub request_id: Option<String>, pub request_hash: Option<String>,
      pub background: Option<String>, pub fit: Option<String>, pub provider_size: Option<String>,
      pub saved_size: Option<String>, pub sha256: Option<String>, pub alpha: Option<AlphaStats>,
      pub connection_source: Option<String>, pub error: Option<EngineError>, pub engine: Option<String> }
  pub fn request_hash(req: &ImageRequest, fit: Fit) -> Result<String, EngineError>;
  pub enum Existing { Absent, Reusable, Mismatch(String), Corrupt(String) }
  pub fn inspect_existing(output: &Path, record: Option<&Path>, request_hash: &str) -> Existing;
  pub struct Reservation { /* output, record: Option<PathBuf>, lock, part */ }
  pub fn reserve(output: &Path, record: Option<&Path>, lock_dir: &Path) -> Result<Reservation, EngineError>;
  impl Reservation {
      pub fn write_pending(&self, rec: &Record) -> Result<(), EngineError>;
      pub fn publish(self, png: &[u8], rec: Record) -> Result<Record, EngineError>; // .part → 디코딩 확인 → rename → complete 기록
      pub fn fail(self, rec: Record, err: &EngineError);                            // failed 기록, .part 삭제
  }
  impl Drop for Reservation { /* lock과 .part 삭제 */ }
  pub fn write_json_atomic(path: &Path, value: &impl Serialize) -> std::io::Result<()>;
  ```
  `record`가 None인 경우(`record_metadata:false`)에는 기록 파일을 쓰지 않는다. lock은 이때도 `lock_dir`(= metadata/<topic>)에 만든다.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test] fn second_reservation_of_same_output_conflicts() {
    let r1 = reserve(&out, Some(&rec), &lock_dir).unwrap();
    assert_eq!(reserve(&out, Some(&rec), &lock_dir).unwrap_err().category, Category::Conflict);
    drop(r1);
    assert!(reserve(&out, Some(&rec), &lock_dir).is_ok());
}
#[test] fn stale_lock_from_dead_process_is_reclaimed() { /* lock JSON {"pid": 4294967000, ...} → reserve 성공 */ }
#[test] fn publish_never_overwrites_existing_file() { /* out을 미리 만들어 두면 publish 오류, 기존 바이트 그대로 */ }
#[test] fn publish_writes_complete_record_with_hash() { /* status "complete", sha256 == sha256(png), saved_size */ }
#[test] fn interrupted_write_leaves_no_final_file() { /* Reservation을 publish 없이 drop → out 없음, .part 없음, lock 없음 */ }
#[test] fn inspect_rules() {
    // complete + 같은 hash + 같은 sha → Reusable
    // complete + 다른 hash → Mismatch
    // complete + 이미지 바이트 변조 → Corrupt
    // pending 기록 + 같은 hash + 디코딩 가능한 이미지 → Reusable (rename 후 완료 기록 전에 중단된 경우)
    // 기록 없음 또는 status 없는 Python 기록 + 디코딩 가능한 이미지 → Reusable (호환)
    // Python 기록 + 깨진 이미지 → Corrupt
    // 이미지 없음 (failed/pending 기록 유무와 무관) → Absent
}
#[test] fn request_hash_changes_with_prompt_model_size_refs_background_fit() { /* 각 필드 하나씩 바꿔 해시가 달라짐; 참조 파일 내용이 바뀌어도 달라짐 */ }
```

`request_hash`는 `{prompt, image_model, size, quality, action, references:[파일 sha256…], background, fit}`을 키 정렬 JSON으로 만든 뒤 sha256(hex)을 계산한다. lock 파일 이름은 `<이미지 파일명>.lock`이고 내용은 `{"pid", "request_id", "created_at"}`이다. 이미 있으면 내용을 읽어 pid가 살아 있지 않을 때만(Windows: `OpenProcess` + `GetExitCodeProcess == STILL_ACTIVE`; 그 밖: `kill(pid, 0)`) 지우고 한 번 다시 시도한다. `.part` 이름은 `.<파일명>.<uuid>.part`(출력 폴더)이다. 공개 순서: create_new로 .part 쓰기 → 다시 디코딩해 크기 확인 → `std::fs::hard_link(part, out)`(대상이 있으면 실패하므로 덮어쓰지 않음) 후 .part 삭제 → complete 기록. 링크가 지원되지 않는 파일 시스템이면 create_new로 바로 쓴다.

- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** `cargo test -p image-core --test store` PASS.
- [ ] **Step 5: 커밋** `feat(core): reserve outputs and publish image+record as one completed result`.

---

### Task 7: 한 장 생성 흐름 + 새 키 사전 검증 (검토 P1-04)

**Files:**
- Create: `crates/image-core/src/generate.rs`
- Test: `crates/image-core/tests/generate.rs`

**Interfaces:**
- Consumes: Task 3–6.
- Produces:
  ```rust
  pub struct JobContext { pub http: Arc<Http>, pub conn: Arc<Connection>, pub record_metadata: bool }
  pub struct JobSpec { pub request: ImageRequest, pub fit: Fit, pub output: PathBuf, pub record: Option<PathBuf>,
                       pub lock_dir: PathBuf, pub resume: bool }
  pub enum JobOk { Generated { output: PathBuf }, Skipped { output: PathBuf } }
  pub fn validate_request(req: &ImageRequest) -> Result<(), EngineError>;   // 호출 전 검사
  pub async fn generate_one(ctx: &JobContext, job: &JobSpec, cancel: &CancellationToken) -> Result<JobOk, EngineError>;
  ```

- [ ] **Step 1: 실패하는 테스트 작성** (FakeProvider 사용)

```rust
#[tokio::test] async fn generates_exact_size_png_and_complete_record() {
    // 제공자가 1536x1024 PNG를 반환, 요청 1920x1080 → 파일 1920x1080, 기록 status complete,
    // provider_size "1536x1024", fit "cover", response_model 반영, connection_source 기록, 키 문자열 없음
}
#[tokio::test] async fn transparent_request_uses_contain_and_records_alpha() {
    // gpt-image-2.5-sunburst + Background::Transparent, 제공자 1374x1145 RGBA → 1024x1024 contain, record.alpha.transparent_ratio > 0
    // 요청 본문에 "background":"transparent"
}
#[tokio::test] async fn rejects_before_any_http_call() {
    // 각각 FakeProvider.requests()가 비어 있어야 함:
    // 없는 참조 파일 → InvalidRequest "reference image not found: <절대 경로>"
    // 이미지가 아닌 참조 파일(텍스트) → InvalidRequest
    // gpt-image-2.5 + Transparent → Unsupported (메시지에 sunburst/flare 안내)
    // grok + 참조 → Unsupported
    // 지원하지 않는 모델 "gpt-5.5" → Unsupported "Responses API path was removed"
}
#[tokio::test] async fn http_200_without_image_fails_without_file() { /* "<html>" → Err Provider retryable, 출력 없음, 기록 status failed */ }
#[tokio::test] async fn quota_error_is_recorded_and_not_retryable() { /* 403 spending-limit → 기록 error.category "quota" */ }
#[tokio::test] async fn existing_output_without_resume_is_rejected_untouched() { /* 기존 파일 바이트 보존, InvalidRequest "Output already exists; use a new topic or resume: …", HTTP 0건 */ }
#[tokio::test] async fn resume_skips_reusable_and_reports_mismatch() { /* 같은 요청 재실행 → Skipped, HTTP 0건; 프롬프트 변경 → Err Conflict, 파일 보존 */ }
#[tokio::test] async fn cancel_during_request_cleans_up() { /* delay 5s 응답, 100ms 후 cancel → Err Cancelled, 출력·lock·.part 없음, 기록 failed/cancelled */ }
```

흐름: `validate_request` → resume/기존 파일 판정(`inspect_existing`) → `reserve` → pending 기록 → `build_call`·`post_json`(cancel과 `select!`) → 200이 아니면 `classify_http_error` → `extract_image` → `normalize`(CPU 작업은 `spawn_blocking`) → `alpha_stats` → `publish`. 오류가 나면 `fail`을 부른다. 기록의 `engine`은 `"image-core <버전>"`이다. 참조 경로는 호출한 쪽(runner)이 절대 경로로 바꿔서 넘긴다. `validate_request`는 참조 파일이 있는지, `image::ImageReader::open(..).with_guessed_format()?.into_dimensions()`로 읽히는지 확인한다.

- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** `cargo test -p image-core --test generate` PASS.
- [ ] **Step 5: 커밋** `feat(core): run one generation end-to-end with pre-call validation`.

---

### Task 8: 실행기 — spec 해석, single/jobs, 병렬·재시도·타임아웃·취소·진행 이벤트

**Files:**
- Create: `crates/image-core/src/runner.rs`
- Test: `crates/image-core/tests/runner.rs`

**Interfaces:**
- Consumes: Task 7 `generate_one`, Task 3 `resolve_connection`.
- Produces: `run_spec`, `Progress`, `RunOutcome`(공용 인터페이스 표). batch는 Task 9에서 `runner::run_batch`로 붙인다.

결과 JSON은 `tools/ai_image.py`의 `run_single`/`run_jobs` 반환과 키·정렬이 같고, 다음을 더한다: 각 실패에 `error`(EngineError 객체), 최상위 `warnings`(배열, 비어 있으면 생략), `connection_source`. 이벤트 이름과 필드도 Python과 같다(`image_started/finished/failed/skipped`, `job_*`, `retry`(`attempt, retries, delay_sec, reason`), `ts`).

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[tokio::test] async fn single_count_three_names_and_parallelism() {
    // count 3 → outputs [slug_001, slug_002, slug_003].png 정렬, concurrency 3, 제공자가 동시에 3건을 받음(지연 300ms로 확인)
}
#[tokio::test] async fn thirty_images_run_at_once_and_clamp() { /* count 30 → 동시 30; concurrency 99 → 32 */ }
#[tokio::test] async fn transient_then_success_counts_retried() { /* 첫 응답 429, 다음 200 → ok, retried 1, retry 이벤트 1건 (AI_IMAGE_RETRY_BACKOFF_SEC=0 대신 runner 옵션으로 0) */ }
#[tokio::test] async fn permanent_errors_are_not_retried() { /* 400 → 요청 1건, failures[0].error.category "invalid_request" */ }
#[tokio::test] async fn proxy_down_fails_every_image_cleanly() { /* 닫힌 포트, count 5, retries 1 → failures 5, 카테고리 network, 출력·lock 없음 */ }
#[tokio::test] async fn per_image_timeout_is_retryable() { /* job_timeout_sec 0.2, 지연 2s → reason "timed out after 0.2s", 재시도됨 */ }
#[tokio::test] async fn jobs_mode_overrides_prefix_suffix_and_collisions() {
    // prefix/suffix가 "\n"으로 결합된 prompt를 제공자가 받음; job별 image_model/size/action override;
    // id "a/b"와 "a_b" → 최상위 error "job IDs collide after filename normalization"; 출력은 output/<topic>/<id slug>.png
}
#[tokio::test] async fn dry_run_makes_no_calls_and_no_dirs() { /* single·jobs 모두 planned_outputs, 디렉터리 생성 없음 */ }
#[tokio::test] async fn validation_errors_match_python_shape() {
    // prompt 없음 → {"ok":false,"error":"spec.prompt is required when mode=single"}, exit 2
    // 잘못된 mode → exit 1 "unknown mode 'x'; use 'single', 'jobs' or 'batch'"
    // profile / mask / keep_outside_mask / remove_background:true → exit 2, error_category "unsupported", HTTP 0건
    // background "rainbow", fit "stretch" → invalid_request
    // 연결 설정 없음(MapEnv 빈 값) → exit 2, 네트워크 호출 0건
}
#[tokio::test] async fn model_key_is_ignored_with_warning() { /* "model":"gpt-5.5" → warnings에 안내 1건, 생성은 진행 */ }
#[tokio::test] async fn cancel_stops_new_work_and_keeps_finished() { /* count 4, concurrency 2, 첫 2장 완료 후 cancel → outputs 2, failures에 cancelled */ }
```

spec 해석 규칙(Python과 같음): `topic` 기본값은 single이 `"ai-image-request"`, jobs가 `"ai-image-jobs"`이다. `count`는 `max(1, int)`이고 `action` 기본값은 `"generate"`이다. `reference_images`와 `topic_root`의 상대 경로는 현재 작업 폴더 기준이다. jobs에서 job의 `reference_images`가 null이면 spec 값을 쓰고 `[]`이면 참조 없이 생성한다. `record_metadata:false`이면 기록을 쓰지 않는다. 숫자 키가 숫자가 아니면 `"job_timeout_sec, concurrency, and retries must be numbers"`를 낸다. 새 키 `background`(`auto` 기본), `fit`(투명이면 `contain`, 아니면 `cover`)는 jobs의 job별 override도 받는다. `profile`, `mask`, `keep_outside_mask`, `remove_background:true`는 Unsupported로 거부하고 메시지에 지원 예정 단계(2 또는 4)를 적는다. 백오프는 `min(60, B·2^(n-1)) + U(0, B/2)`이고, B는 `RunOptions`(테스트에서 0, CLI에서는 환경변수)로 받는다. 시간 제한은 장 단위로 `tokio::time::timeout`을 쓰며 재시도마다 새로 잰다. 동시 실행은 `Semaphore(workers)`이고 workers는 `min(concurrency, 장수)`이다. 취소되면 대기 중인 장은 시작하지 않고 `cancelled` 실패로 보고한다. 이 때문에 `run_spec`은 `RunOptions { backoff_base_sec: f64 }`를 받는 `run_spec_with`를 내부에 두고, 공개 `run_spec`은 환경변수에서 B를 읽는다.

- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** `cargo test -p image-core --test runner` PASS(전체 10초 이내).
- [ ] **Step 5: 커밋** `feat(core): run single and jobs specs in parallel with retries, timeouts and cancel`.

---

### Task 9: batch 모드 + Python 호환 난수

**Files:**
- Create: `crates/image-core/src/batch.rs`
- Modify: `crates/image-core/src/runner.rs`(mode "batch" 연결)
- Test: `crates/image-core/tests/batch.rs`

**Interfaces:**
- Produces: `pub struct PyRandom`(`PyRandom::from_str_seed(&str)`, `randbelow(n: u64) -> u64`); `pub fn plan_batch(preset: &Path, count: u32, seed: i64, topic: Option<&str>, image_model: Option<&str>, workspace: &Path) -> Result<Vec<BatchPlan>, EngineError>`; `BatchPlan { id, prompt, category, image_model, output }`(Serialize, gen_batch dry-run 항목과 같은 키).

Python `random.Random(f"{seed}:{slug}:{index}")`와 같은 수열을 만들어야 기존 preset 데이터셋의 프롬프트가 같다(resume 호환). 시드 알고리즘(CPython 3, version 2):

```
a = s.encode("utf-8"); a = a + sha512(a).digest()      # 바이트열
n = int.from_bytes(a, "big")
key = n를 32비트 단위로 최하위부터 나눈 리스트 (n == 0이면 [0])
MT19937 init_by_array(key)
getrandbits(k≤32) = genrand_uint32() >> (32 - k)
getrandbits(k>32) = 32비트 단위로 하위부터 채우고, 마지막 조각은 >> (32 - 남은 비트)
randbelow(n): k = n.bit_length(); r = getrandbits(k); while r >= n: r = getrandbits(k)
weighted_category: pick = randbelow(total); choice(templates) = templates[randbelow(len)]
```

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test] fn pyrandom_matches_cpython() {
    // 기대값은 .venv Python으로 미리 계산해 상수로 박는다:
    //   r = random.Random("42:extraction-rpg:1"); [r._randbelow(n) for n in (10, 1000, 7, 2**40)]
    let mut r = PyRandom::from_str_seed("42:extraction-rpg:1");
    assert_eq!([r.randbelow(10), r.randbelow(1000), r.randbelow(7), r.randbelow(1 << 40)], EXPECTED);
}
#[test] fn plan_matches_gen_batch_dry_run() {
    // presets/extraction-rpg.json, count 5, seed 42 → id/prompt/category가 Python dry-run(아래 Step 0에서 저장한 fixture)과 같음
}
#[tokio::test] async fn batch_runs_plans_and_stops_after_max_failures() { /* 항상 400 → 실패 20건 이후 남은 계획은 시작하지 않음 */ }
```

- [ ] **Step 0:** `.venv/Scripts/python.exe scripts/gen_batch.py presets/extraction-rpg.json --count 5 --seed 42 --dry-run --topic-root <tmp>` 결과의 `planned_outputs`에서 id·prompt·category만 뽑아 `crates/image-core/tests/fixtures/batch_extraction_rpg_5.json`으로 저장한다. EXPECTED도 같은 방식으로 계산한다.
- [ ] **Step 2:** FAIL. **Step 3:** 구현. batch 결과 키는 ai_image `run_batch`와 같다(`stdout_tail`/`stderr_tail`은 내지 않음, 의도된 차이). preset 경로가 상대 경로면 현재 폴더를 먼저 보고, 없으면 실행 파일 위쪽 폴더들 가운데 `presets/`를 가진 저장소 루트를 기준으로 찾는다. 이벤트는 `mode:"batch"`의 `job_*`이다.
- [ ] **Step 4:** PASS. **Step 5: 커밋** `feat(core): port preset batch planning with CPython-compatible RNG`.

---

### Task 10: `image-cli` 실행 파일

**Files:**
- Create: `crates/image-cli/Cargo.toml`(bin name `image-cli`), `src/main.rs`, `crates/image-core/src/schema.rs`
- Test: `crates/image-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `run_spec`, `list_models`, `resolve_connection`, `schema::schema_json() -> Value`, `schema::examples_json() -> Value`.
- Produces: 실행 파일 `target/release/image-cli.exe`. 인자는 `--spec <file>` | `--json <string>` | `--schema` | `--example` | `--list-models`이고 서로 배타적이다. 아무것도 없으면 stdin을 읽는다.

- [ ] **Step 1: 실패하는 테스트 작성** (`env!("CARGO_BIN_EXE_image-cli")`, FakeProvider, `env_clear` 후 필요한 변수만 설정)

```rust
#[test] fn stdout_has_exactly_one_json_line_and_progress_goes_to_stderr() { /* stdout 줄 1개, stderr 각 줄이 JSON */ }
#[test] fn exit_codes() { /* 성공 0, 실패 포함 2, 깨진 JSON 1("invalid JSON spec: …"), 빈 stdin 1 */ }
#[test] fn accepts_utf8_bom_spec_file() {}
#[test] fn schema_has_python_keys_plus_new_keys() {
    // Python SCHEMA의 모든 키 포함 + background, fit, mask, remove_background, keep_outside_mask, profile
    // 새 키 description에 지원 상태("stage 2", "stage 4")를 적음; topic_root default는 현재 폴더
}
#[test] fn list_models_filters_image_ids() {
    // 제공자 /v1/models: ["gpt-image-2.5","gpt-5.5","gemini-3.1-flash-image","grok-imagine-image-2.0"]
    // → available_models 3개, models[].reference_images grok false; 출력 파일 없음
}
#[test] fn list_models_without_connection_config_fails_offline() { /* exit 1, error에 CLIPROXY_BASE_URL */ }
#[test] fn key_never_appears_in_any_output() { /* 403 응답 본문에 키를 되돌려 주는 제공자 → stdout·stderr·기록 파일 어디에도 키 문자열 없음 */ }
```

`--list-models` 결과 키는 Python과 같다(`ok, base_url, key_configured, available_models, models, error?`). Ctrl+C는 `tokio::signal::ctrl_c`로 CancellationToken을 취소하고, 결과 JSON을 낸 뒤 종료 코드 2로 끝낸다. 진행 이벤트는 stderr에 줄 단위로 쓰고 매번 flush한다.

- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** `cargo test --workspace` PASS. `cargo build --release -p image-cli` 성공.
- [ ] **Step 5: 커밋** `feat(cli): add image-cli with the ai_image.py JSON contract`.

---

### Task 11: Python 동등성 비교 + 의도된 차이 문서

**Files:**
- Create: `crates/image-cli/tests/parity.rs`, `docs/RUST_PARITY.md`
- Modify: `README.md`(Rust CLI 1단계 사용법 짧게), `docs/SSOT.md`(이전 상태 한 줄)

- [ ] **Step 1: 테스트 작성** — `.venv/Scripts/python.exe`(또는 `.venv/bin/python`)가 없으면 건너뛴다(`eprintln!` 후 return). 같은 FakeProvider(1536x1024 PNG 반환)에 대해 같은 spec을 Python `tools/ai_image.py`와 Rust `image-cli`로 각각 다른 임시 topic_root에서 실행한다. 시나리오는 single count 2, jobs 2개(참조 1장 포함, prefix/suffix), dry-run 3모드(batch는 extraction-rpg count 3 seed 42), 400 실패 1건, resume 재실행이다.

비교(정규화: topic_root 접두어 → `<ROOT>`, `elapsed_sec`·`ts` 제거, Rust 전용 키 `warnings`·`connection_source`·`failures[].error` 제거, batch의 `stdout_tail`·`stderr_tail` 제거):
```rust
assert_eq!(normalize(py_result), normalize(rs_result));
assert_eq!(list_files(py_root), list_files(rs_root));           // 상대 경로 집합
for each image: 크기(width,height) 같음
for each record: python 키 집합 ⊆ rust 키 집합, 공통 키 값 같음(output 경로는 정규화)
assert_eq!(provider_bodies(py), provider_bodies(rs));            // 제공자가 받은 JSON 본문이 같음(참조 data URL 포함)
```
- [ ] **Step 2:** `cargo test -p image-cli --test parity -- --nocapture`를 실행한다. 다른 점이 나오면 Rust를 Python에 맞추거나, 의도된 차이라면 정규화 목록과 `docs/RUST_PARITY.md`에 함께 올린다. 두 곳은 같은 목록이어야 한다.
- [ ] **Step 3:** `docs/RUST_PARITY.md` 작성: 하드코딩 주소 제거, Responses 경로 제거(`model` 무시 경고), 리다이렉트 차단, 기록 새 필드와 `status`, 실패 시 failed 기록, 같은 경로 동시 실행 conflict, resume 해시 검증, 새 키, 실패의 `error` 객체, batch의 tail 키 없음, preset 상대 경로 해석, batch 진행 이벤트 추가. 항목마다 이유 한 줄과 spec 절을 적는다.
- [ ] **Step 4:** Python 테스트 전체(`.venv/Scripts/python.exe -m unittest discover -s tests`)와 `cargo test --workspace`를 모두 통과시킨다.
- [ ] **Step 5: 커밋** `test: compare Rust image-cli against the Python CLI on a shared fixture`.

---

### Task 12: 실제 프록시 확인 (2장) + 단계 상태 기록

비용이 드는 실제 호출이다. 각 명령은 dry-run을 먼저 실행한 뒤 실제로 실행한다. 대상은 `topic_root: E:/ai-image-generator`이다.

- [ ] **Step 1:** `target/release/image-cli --list-models`로 `ok:true`와 실제 모델 목록을 확인한다(Task 1의 `CLIPROXY_BASE_URL` 사용).
- [ ] **Step 2:** `{"prompt":"A blue ceramic mug with a gold handle on a plain table, product photo","topic":"rust-cli-smoke-20261018","size":"1024x1024","quality":"low","image_model":"gpt-image-2.5"}`을 dry-run 후 실행한다. 결과가 `ok:true`이고, 파일이 1024x1024이며, 기록 status가 complete인지, 이미지가 실제로 컵인지 직접 열어 확인한다.
- [ ] **Step 3:** `{"prompt":"A blue ceramic mug with a gold handle, isolated product cutout","topic":"rust-cli-smoke-transparent-20261018","size":"1024x1024","quality":"low","image_model":"gpt-image-2.5-sunburst","background":"transparent"}`을 실행한다. 결과 기록의 `fit:"contain"`, `alpha.transparent_ratio > 0`, 네 모서리 알파 0을 확인하고, 손잡이가 잘리지 않았는지 직접 본다.
- [ ] **Step 4:** spec §10 표의 단계 1 칸에 날짜·결과·사용 모델(요청 ID)을 적고, `docs/SSOT.md`에 Rust CLI 실검증 날짜를 기록한다.
- [ ] **Step 5: 커밋** `docs: record stage 1 Rust CLI verification`.
