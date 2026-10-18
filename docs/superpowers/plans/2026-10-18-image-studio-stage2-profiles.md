# Image Studio 단계 2: 연결 프로필·공식 API 어댑터 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 연결을 "프로필"(공급자 종류 + 주소 + 키 참조)로 저장·선택하고, CLIProxyAPI 외에 OpenAI·Gemini·xAI 공식 API와 기타 OpenAI 호환 서버를 같은 엔진으로 호출한다. 키는 Windows 자격 증명 관리자에만 둔다.

**Architecture:** `image-core`에 `profiles`(파일 저장소), `secrets`(키 보관 추상화), `adapter`(공급자 종류별 요청·응답·모델 목록) 모듈을 더한다. `Connection`은 공급자 종류와 인증 방식을 함께 들고 다니며, `provider::build_call`은 종류별 어댑터로 나뉜다. CLI는 `--profile`과 `profile` 하위 명령을 얻는다. 앱(단계 3)은 같은 API를 쓴다.

**Tech Stack:** 단계 1과 같음 + `keyring-core = "1"`(1.0.1), Windows 대상 `windows-native-keyring-store = "1"`(1.1.0). 테스트는 `keyring_core::mock`이 아니라 우리 `MemoryStore`를 쓴다(전역 기본 저장소를 건드리지 않기 위해).

**Spec:** `docs/superpowers/specs/2026-10-18-image-studio-rust-app-design.md` §5.2(해석 순서), §6(프로필·연결 확인·기능표·요청 형식), §5.4(오류 구조). 사실 근거: `docs/superpowers/research/2026-10-18-provider-keyring-tauri-facts.md`(2026-10-18 공식 문서 조사·교차 검증, 코드 예시 포함).

## Global Constraints

- 단계 1 Global Constraints 전부 유지(키 평문 금지, 덮어쓰기 금지, 모델 대체 금지, 리다이렉트 금지, 가드 점검 후 커밋).
- 프로필 파일 위치: 환경변수 `IMAGE_STUDIO_HOME`이 있으면 그 폴더, 없으면 `%APPDATA%\ImageStudio`. 파일 `profiles.json`, `status.json`. 키는 이 폴더에 절대 쓰지 않는다.
- 자격 증명 관리자 항목: service `ImageStudio`(환경변수 `IMAGE_STUDIO_KEYRING_SERVICE`로 바꿀 수 있음, 테스트 전용 용도), user = 프로필 id. 대상 이름은 `"{id}.ImageStudio"`.
- 해석 순서(spec §5.2): spec `profile` 또는 `--profile` → 프로세스 `CLIPROXY_BASE_URL` → 사용자 `CLIPROXY_BASE_URL` → 기본 프로필 → 오류. 명시한 프로필이 없거나 불완전하면 다른 출처의 주소·키를 빌리지 않고 오류.
- 프로필 종류 문자열: `cliproxy`, `openai`, `gemini`, `xai`, `openai_compatible`.
- 기본 주소: openai `https://api.openai.com/v1`, gemini `https://generativelanguage.googleapis.com`, xai `https://api.x.ai/v1`. cliproxy·openai_compatible은 사용자 입력 필수.
- `/v1` 자동 보정은 cliproxy·openai·xai·openai_compatible에만. gemini 주소는 쿼리·인증정보 금지 규칙만 적용하고 경로를 바꾸지 않는다.
- 키 필수: openai·gemini·xai. cliproxy·openai_compatible은 키 없이도 허용(단계 1과 같음).
- 기본 동시 실행: cliproxy·openai_compatible 30, openai·gemini·xai 4. 프로필의 `concurrency`가 있으면 그 값, spec `concurrency`가 있으면 그 값이 우선.
- 공식 API 직접 호출은 사용자 키가 없어 이번에 실검증하지 않는다. 문서 근거로 구현하고 기능표에 "문서 근거, 미실측"으로 표시한다.

## Review Focus

1. 프로필 이름에 공백·한국어·점(`.`)이 섞인 경우: id는 생성값(`p-` + 8자리 hex)이라 자격 증명 대상 이름이 겹치지 않는다. 이름 중복은 대소문자 무시로 거부. 테스트: Task 2.
2. `profiles.json`이 깨졌거나(사용자 편집, 반쯤 쓰다 중단) 버전이 다른 경우: 생성을 막지 말고 "프로필 파일을 읽을 수 없음" 오류를 정확히 내며 파일을 덮어쓰지 않는다. 테스트: Task 2.
3. 기본 프로필을 지웠거나 기본 프로필의 키가 자격 증명 관리자에서 사라진 경우: 네트워크 호출 전 `invalid_request`로 "키가 없습니다(프로필 X)" 오류. 테스트: Task 3.
4. Gemini 프로필에 `gpt-image-2.5`를 요청하는 등 종류와 모델이 안 맞는 경우: 유료 호출 전 `unsupported`. 테스트: Task 5.
5. 공용 인터넷의 `http://` 주소: 저장은 되지만 결과에 경고가 붙는다(루프백·사설망·Tailscale 100.64/10·`*.ts.net`은 경고 없음). 테스트: Task 2.

---

## File Structure

```
crates/image-core/src/
  profiles.rs     # ProviderKind, Profile, ProfileStore(파일), 이름/id 규칙, http 경고, status.json
  secrets.rs      # SecretStore trait, KeyringStore(Windows), MemoryStore(테스트·비Windows)
  adapter.rs      # 종류별 요청 빌드·응답 해석·모델 목록 (provider.rs의 build_call/list_models가 위임)
  connection.rs   # Connection에 kind/auth/profile 추가, resolve_connection_with(profile 지정)
  http.rs         # Auth(Bearer | GoogApiKey) 지원
  provider.rs     # build_call(conn, req), list_models(http, conn) — 시그니처 변경
  check.rs        # 연결 확인 보고서(서버 접근 → 인증 → 모델 목록), 마지막 생성 결과 기록
  runner.rs       # spec profile 지원, 종류별 기본 동시성, 생성 결과 status.json 갱신
crates/image-core/tests/{profiles.rs, adapters.rs, check.rs, runner_profiles.rs}
crates/image-cli/src/main.rs   # --profile, profile 하위 명령
crates/image-cli/tests/profile_cli.rs
docs/RUST_PARITY.md, README.md, INSTALL.md, skills/ai-image-generator/SKILL.md  # 사용법 갱신
```

## 공용 인터페이스

```rust
// profiles.rs
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)] #[serde(rename_all = "snake_case")]
pub enum ProviderKind { Cliproxy, Openai, Gemini, Xai, OpenaiCompatible }
impl ProviderKind { pub fn parse(&str) -> Option<Self>; pub fn as_str(self) -> &'static str;
                    pub fn default_base_url(self) -> Option<&'static str>; pub fn key_required(self) -> bool;
                    pub fn default_concurrency(self) -> usize; pub fn label(self) -> &'static str; } // 화면용 한국어 이름
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Profile { pub id: String, pub name: String, pub kind: ProviderKind, pub base_url: String,
                     #[serde(default, skip_serializing_if = "Option::is_none")] pub concurrency: Option<u32> }
pub struct ProfileStore { /* dir */ }
impl ProfileStore {
    pub fn open(dir: PathBuf) -> ProfileStore;                    // 파일이 없어도 됨
    pub fn default_dir(env: &dyn EnvSource) -> Option<PathBuf>;   // IMAGE_STUDIO_HOME → %APPDATA%\ImageStudio
    pub fn load(&self) -> Result<ProfileFile, EngineError>;       // 없으면 빈 파일
    pub fn add(&self, name: &str, kind: ProviderKind, base_url: Option<&str>, concurrency: Option<u32>) -> Result<Profile, EngineError>;
    pub fn update(&self, name_or_id: &str, patch: ProfilePatch) -> Result<Profile, EngineError>;
    pub fn remove(&self, name_or_id: &str) -> Result<Profile, EngineError>;
    pub fn set_default(&self, name_or_id: Option<&str>) -> Result<(), EngineError>;
    pub fn find(&self, name_or_id: &str) -> Result<Profile, EngineError>;
    pub fn record_generation(&self, profile_id: &str, outcome: &GenerationStatus) -> Result<(), EngineError>;
    pub fn generation_status(&self) -> Result<HashMap<String, GenerationStatus>, EngineError>;
}
#[derive(Serialize, Deserialize)] pub struct ProfileFile { pub version: u32 /*=1*/, pub default: Option<String>, pub profiles: Vec<Profile> }
#[derive(Default)] pub struct ProfilePatch { pub name: Option<String>, pub base_url: Option<String>, pub concurrency: Option<Option<u32>> }
#[derive(Serialize, Deserialize, Clone)] pub struct GenerationStatus { pub ok: bool, pub at: f64, pub model: String,
    pub category: Option<Category>, pub provider_code: Option<String> }
pub fn transport_warning(base_url: &str) -> Option<String>;   // 공용 인터넷 http:// 이면 경고 문구

// secrets.rs
pub trait SecretStore: Send + Sync {
    fn get(&self, profile_id: &str) -> Result<Option<Secret>, EngineError>;
    fn set(&self, profile_id: &str, key: &str) -> Result<(), EngineError>;
    fn delete(&self, profile_id: &str) -> Result<(), EngineError>;   // 없어도 Ok
}
pub struct KeyringStore { service: String }   // KeyringStore::system(env) — service는 IMAGE_STUDIO_KEYRING_SERVICE 또는 "ImageStudio"
pub struct MemoryStore { /* Mutex<HashMap> */ }

// http.rs
pub enum AuthScheme { Bearer, GoogApiKey }
// post_json/get의 key 인자를 auth: (AuthScheme, Option<&Secret>)로 바꾼다:
pub async fn post_json(&self, url: &str, body: &Value, auth: Auth<'_>) -> Result<(u16, Vec<u8>), EngineError>;
pub async fn get(&self, url: &str, auth: Auth<'_>) -> Result<(u16, Vec<u8>), EngineError>;
#[derive(Clone, Copy)] pub struct Auth<'a> { pub scheme: AuthScheme, pub key: Option<&'a Secret> }

// connection.rs
pub struct Connection { pub kind: ProviderKind, pub base_url: String, pub api_key: Option<Secret>,
                        pub source: String,            // "process_env" | "user_env" | "profile:<name>"
                        pub profile_id: Option<String>, pub concurrency_default: usize }
impl Connection { pub fn auth(&self) -> Auth<'_>; }
pub struct Resolver<'a> { pub env: &'a dyn EnvSource, pub profiles: Option<&'a ProfileStore>, pub secrets: &'a dyn SecretStore }
pub fn resolve(r: &Resolver, explicit_profile: Option<&str>) -> Result<Connection, EngineError>;
// 기존 resolve_connection(env)는 profiles=None, secrets=MemoryStore로 resolve 호출하는 얇은 래퍼로 남긴다.

// adapter.rs
pub fn model_supported(kind: ProviderKind, model: &str) -> Result<(), EngineError>;     // 종류-모델 불일치 → Unsupported
pub fn supports_references(kind: ProviderKind, model: &str) -> bool;
pub fn supports_native_transparency(kind: ProviderKind, model: &str) -> bool;
pub fn build_call(conn: &Connection, req: &ImageRequest) -> Result<ProviderCall, EngineError>;
pub fn extract_image(kind: ProviderKind, backend: &Backend, body: &[u8]) -> Result<(Vec<u8>, Option<String>), EngineError>;
pub async fn list_models(http: &Http, conn: &Connection) -> Result<Vec<String>, EngineError>;

// check.rs
#[derive(Serialize)] pub struct ConnectionReport { pub profile: Option<String>, pub kind: ProviderKind, pub base_url: String,
    pub key_configured: bool, pub reachable: bool, pub authenticated: bool, pub models_listed: bool,
    pub image_models: Vec<String>, pub last_generation: Option<GenerationStatus>, pub warning: Option<String>,
    pub error: Option<EngineError> }
pub async fn check_connection(http: &Http, conn: &Connection, store: Option<&ProfileStore>) -> ConnectionReport;
```

---

### Task 1: 키 보관소 (`secrets.rs`)

**Files:** Create `crates/image-core/src/secrets.rs`, Test `crates/image-core/tests/secrets.rs`. Modify `crates/image-core/Cargo.toml`(keyring-core 1, cfg(windows) windows-native-keyring-store 1).

- [ ] **Step 1: 테스트 작성**
```rust
#[test] fn memory_store_roundtrip() { set("p-1","sk-a") → get == Some("sk-a"); delete → get == None; delete again → Ok }
#[test] fn memory_store_never_debug_prints_key() { format!("{:?}", store.get(..).unwrap()) == "Some(***)" }
#[cfg(windows)] #[test] fn keyring_store_roundtrip_in_isolated_service() {
    // service = format!("ImageStudioTest-{uuid}") — 실제 자격 증명 관리자에 쓰고 반드시 지운다
    let s = KeyringStore::with_service(&svc); s.set("p-test", "sk-x").unwrap();
    assert_eq!(s.get("p-test").unwrap().unwrap().expose(), "sk-x");
    s.delete("p-test").unwrap(); assert!(s.get("p-test").unwrap().is_none());
}
#[cfg(windows)] #[test] fn keyring_store_rejects_empty_and_too_long_keys() { "" → InvalidRequest; 1300자 → InvalidRequest("키가 너무 깁니다") }
```
- [ ] **Step 2:** `cargo test -p image-core --test secrets` → 컴파일 실패.
- [ ] **Step 3:** 구현. `KeyringStore`는 호출마다 `windows_native_keyring_store::Store::new()`로 만든 저장소에서 `keyring_core::Entry::new_with_credential`이 아니라 저장소의 `build(service, user, None)`(또는 문서상 동등 API)로 항목을 만든다 — **전역 `set_default_store`를 쓰지 않는다**(앱·테스트가 같은 프로세스에서 다른 저장소를 쓸 수 있게). 구현 중 keyring-core 1.0.1에서 저장소 인스턴스로 Entry를 만드는 정확한 API를 docs.rs로 확인하고, 없으면 `set_default_store`를 `OnceLock`으로 한 번만 호출하는 방식으로 바꾸고 Ruling을 남긴다. 키는 앞뒤 공백을 제거해 저장, 빈 키 거부. `NoEntry` → `Ok(None)`. 그 밖의 오류 → `Internal`("자격 증명 관리자 오류: …"), 메시지에 키 값 없음. 비Windows에서 `KeyringStore::system`은 `Unsupported` 오류를 낸다.
- [ ] **Step 4:** PASS. **Step 5:** 커밋 `feat(core): store API keys in Windows Credential Manager`.

### Task 2: 프로필 저장소 (`profiles.rs`)

**Files:** Create `crates/image-core/src/profiles.rs`, Test `crates/image-core/tests/profiles.rs`.

- [ ] **Step 1: 테스트 작성** (모두 `tempfile` 폴더를 `ProfileStore::open`에 준다)
```rust
#[test] fn add_list_default_remove() {
    let p = store.add("집 PC", ProviderKind::Cliproxy, Some("http://100.98.54.122:8317"), None).unwrap();
    assert!(p.id.starts_with("p-") && p.id.len() == 10);
    assert_eq!(p.base_url, "http://100.98.54.122:8317/v1");
    store.set_default(Some("집 PC")).unwrap(); assert_eq!(store.load().unwrap().default, Some(p.id.clone()));
    store.remove(&p.id).unwrap(); assert_eq!(store.load().unwrap().default, None);  // 기본 프로필 삭제 시 기본값도 해제
}
#[test] fn official_kinds_fill_default_urls() { openai → "https://api.openai.com/v1"; gemini → "https://generativelanguage.googleapis.com"(뒤에 /v1 없음); xai → "https://api.x.ai/v1" }
#[test] fn cliproxy_requires_url() { add("x", Cliproxy, None) → InvalidRequest }
#[test] fn names_are_unique_case_insensitive_and_trimmed() { add("Home") 후 add(" home ") → Conflict; 빈 이름 → InvalidRequest }
#[test] fn update_renames_and_changes_url_and_concurrency() {}
#[test] fn corrupt_file_is_reported_and_left_untouched() {
    std::fs::write(dir/"profiles.json", "{not json"); assert_eq!(store.load().unwrap_err().category, Category::InvalidRequest);
    assert!(store.add(..).is_err()); assert_eq!(read(dir/"profiles.json"), "{not json");
}
#[test] fn unknown_version_is_rejected() { {"version": 9, ...} → InvalidRequest "프로필 파일 버전 9" }
#[test] fn file_never_contains_keys() { profiles.json 내용에 "key" 필드 없음 (직렬화 형태 고정: version, default, profiles[id,name,kind,base_url,concurrency?]) }
#[test] fn transport_warning_rules() {
    for ok in ["https://api.openai.com/v1","http://127.0.0.1:8317/v1","http://localhost:1/v1","http://10.0.0.5/v1","http://172.20.1.1/v1",
               "http://192.168.0.2/v1","http://100.98.54.122:8317/v1","http://desk.tail1234.ts.net:8317/v1","http://[::1]:1/v1"] { assert!(transport_warning(ok).is_none(), "{ok}") }
    for warn in ["http://example.com/v1","http://8.8.8.8/v1","http://100.200.0.1/v1"] { assert!(transport_warning(warn).is_some(), "{warn}") }
}
#[test] fn generation_status_is_recorded_per_profile() { record_generation(id, ok:false, quota) → generation_status()[id].category == Some(Quota) }
#[test] fn default_dir_prefers_image_studio_home() { MapEnv IMAGE_STUDIO_HOME=X → Some(X); APPDATA=Y → Some(Y\ImageStudio) }
```
- [ ] **Step 2:** FAIL 확인. **Step 3:** 구현. 쓰기는 `store::write_json_atomic`. 동시 수정은 `profiles.json.lock`(create_new, 5초까지 50ms 간격 재시도 후 Conflict)으로 직렬화. id는 uuid v4 앞 8 hex. Tailscale 판정은 100.64.0.0/10(100.64.0.0–100.127.255.255).
- [ ] **Step 4:** PASS. **Step 5:** 커밋 `feat(core): add connection profile store`.

### Task 3: 연결 해석 + 인증 방식 (`connection.rs`, `http.rs`)

**Files:** Modify `connection.rs`, `http.rs`, `provider.rs`(호출부), `generate.rs`, `runner.rs`(source가 String). Test `crates/image-core/tests/http.rs`(기존 갱신), `crates/image-core/tests/connection_profiles.rs`.

- [ ] **Step 1: 테스트 작성**
```rust
#[test] fn explicit_profile_wins_over_env() { env CLIPROXY_* 있음 + resolve(Some("gem")) → kind Gemini, source "profile:gem", key from MemoryStore }
#[test] fn env_beats_default_profile() { env 있음 + default 프로필 있음 + explicit 없음 → source "process_env", kind Cliproxy }
#[test] fn default_profile_used_when_env_missing() { → source "profile:<name>" }
#[test] fn missing_profile_is_an_error_without_fallback() { resolve(Some("nope")) → InvalidRequest "프로필을 찾을 수 없습니다: nope", env가 있어도 }
#[test] fn profile_without_required_key_is_an_error() { openai 프로필, 키 없음 → InvalidRequest "키가 없습니다" + 프로필 이름 }
#[test] fn cliproxy_profile_may_have_no_key() {}
#[test] fn nothing_configured_names_both_options() { 오류 메시지에 "CLIPROXY_BASE_URL"과 "image-cli profile add" }
#[tokio::test] async fn gemini_auth_uses_goog_header_not_bearer() { FakeProvider: x-goog-api-key == "k", Authorization 없음 }
#[test] fn concurrency_default_follows_kind_and_profile() { openai 4, cliproxy 30, profile.concurrency Some(7) → 7 }
```
- [ ] **Step 2:** FAIL. **Step 3:** 구현. 단계 1 테스트(`http.rs`, `provider.rs`, `generate.rs`, `runner.rs`, CLI)를 새 시그니처로 고친다(동작 변화 없음). `resolve_connection(env)`는 프로필 없이 같은 동작.
- [ ] **Step 4:** `cargo test --workspace` 전체 PASS. **Step 5:** 커밋 `feat(core): resolve connections from profiles with per-kind auth`.

### Task 4: 공급자 어댑터 (`adapter.rs`)

**Files:** Create `adapter.rs`; Modify `provider.rs`(build_call/extract_image/list_models가 adapter로 위임), `generate.rs`(validate가 kind 기준 기능표 사용). Test `crates/image-core/tests/adapters.rs`.

규칙(공식 문서, 2026-10-18 조사):
- **cliproxy / openai_compatible:** 단계 1 동작 그대로(GPT·Grok은 `/images/*`, Gemini는 `{root}/v1beta/models/{m}:generateContent` + Bearer). 모든 모델 허용. Grok 참조는 단계 1처럼 거부(프록시 경유 미검증).
- **openai:** `gpt-image-*`만 허용. 생성 `POST {base}/images/generations`, 편집 `POST {base}/images/edits` JSON `images:[{image_url: data URL}]`. 본문은 단계 1 GPT 본문과 같음(`model, prompt, n:1, size(16배수 패딩), quality, output_format:"png"`, 선택 `background`). 응답 `data[0].b64_json`, 응답에 `model` 없음(문서). 모델 목록 `GET {base}/models` → `data[].id` 중 `gpt-image-` 시작만. 투명 지원: `gpt-image-2.5-sunburst`, `gpt-image-2.5-flare`(+ `-2026-09-08` 스냅샷) — 접두어 일치로 판정.
- **gemini:** `gemini-*image*`만 허용. 주소 `{base}/v1beta/models/{m}:generateContent`, 헤더 `x-goog-api-key`. 본문은 단계 1 Gemini 본문 그대로. 응답 해석 단계 1과 같되 `thought: true`인 part는 건너뛴다. `finishReason`이 `IMAGE_SAFETY|IMAGE_PROHIBITED_CONTENT|IMAGE_OTHER|NO_IMAGE|IMAGE_RECITATION`이고 이미지가 없으면 `provider` + `provider_code`=finishReason, `retryable:false`. 모델 목록 `GET {base}/v1beta/models?pageSize=1000[&pageToken=…]`를 `nextPageToken`이 없을 때까지(최대 10쪽) 반복, `name`에서 `models/` 제거, `supportedGenerationMethods`에 `generateContent`가 있고 id에 `image`가 들어간 것만.
- **xai:** `grok-imagine-*`만 허용. 생성 `POST {base}/images/generations` 본문 `{model, prompt, n:1, response_format:"b64_json", aspect_ratio, resolution}`; `aspect_ratio`는 요청 크기에 가장 가까운 값(`1:1,3:4,4:3,9:16,16:9,2:3,3:2,1:2,2:1,21:9,5:2` 중, 21:9와 5:2는 `grok-imagine-image-2.0`만), `resolution`은 긴 변 ≤1536이면 `"1k"`, 아니면 `"2k"`. `quality`는 `grok-imagine-image-2.0`에만: low→`"low"`, medium·high→`"medium"`. 편집 `POST {base}/images/edits` JSON: 참조 1장이면 `"image":{"url": data URL}`, 2~5장이면 `"images":[{"url":…},…]`(2.0만 5장, 그 밖은 1장 초과 시 Unsupported). 마스크·투명 없음. 응답 `data[0].b64_json` + `mime_type`(JPEG일 수 있음 → normalize가 PNG로 변환), 최상위 `model`이 있으면 response_model. 모델 목록 `GET {base}/models` → `data[].id` 중 `grok-imagine-image` 시작.

- [ ] **Step 1: 테스트 작성** — 종류별로 FakeProvider에 실제 호출을 보내 경로·헤더·본문을 확인한다.
```rust
#[tokio::test] async fn openai_generation_and_edit_bodies() {}
#[tokio::test] async fn openai_rejects_gemini_model_before_call() { Unsupported, 요청 0건 }
#[tokio::test] async fn gemini_uses_goog_key_and_skips_thought_parts() {}
#[tokio::test] async fn gemini_safety_block_is_not_retryable() { finishReason IMAGE_SAFETY, 이미지 없음 → provider_code "IMAGE_SAFETY", retryable false }
#[tokio::test] async fn gemini_model_list_follows_pages() { 1쪽 nextPageToken "t2" → 2쪽 → ids 합침, generateContent 없는 모델 제외 }
#[tokio::test] async fn xai_generation_maps_size_to_aspect_and_resolution() { 1920x1080 → "16:9","2k"; 1024x1024 → "1:1","1k"; quality high on 2.0 → "medium"; grok-imagine-image에는 quality 없음 }
#[tokio::test] async fn xai_edit_single_and_multi() { 1장 → image.url data URL; 3장 + 2.0 → images[3]; 3장 + grok-imagine-image → Unsupported }
#[tokio::test] async fn xai_jpeg_output_is_saved_as_png() { mime_type image/jpeg b64 → 저장 파일 PNG }
#[tokio::test] async fn cliproxy_behaviour_is_unchanged() { 단계 1 provider 테스트와 같은 본문 }
#[test] fn capability_table() { (Openai,"gpt-image-2.5-sunburst-2026-09-08") 투명 O; (Xai,"grok-imagine-image-2.0") 참조 O, 투명 X; (Gemini,..) 투명 X; (Cliproxy,"grok-imagine-image") 참조 X }
```
- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** `cargo test --workspace` PASS. **Step 5:** 커밋 `feat(core): add OpenAI, Gemini and xAI adapters`.

### Task 5: 실행기·연결 확인 (`runner.rs`, `check.rs`)

**Files:** Create `check.rs`; Modify `runner.rs`. Test `crates/image-core/tests/runner_profiles.rs`, `crates/image-core/tests/check.rs`.

- runner: `RunOptions`에 `profiles: Option<Arc<ProfileStore>>`, `secrets: Arc<dyn SecretStore>`를 더한다(`run_spec`은 `ProfileStore::default_dir`·`KeyringStore::system`으로 채움). spec `profile` 키를 지원하고(단계 1의 unsupported 거부 제거), 종류-모델 불일치·기능 불가는 `build_request`에서 사전 거부. 동시성 기본값은 `conn.concurrency_default`. 실제 실행 후 연결이 프로필에서 왔으면 `record_generation`(성공 1장 이상이면 ok:true, 아니면 첫 실패의 category/provider_code).
- check: 서버 접근(전송 오류면 reachable false) → 인증(401/403이면 authenticated false, 단 quota면 authenticated true) → 모델 목록 → 이미지 모델 필터. 유료 호출 없음. `last_generation`은 status.json에서. `warning`은 `transport_warning`.

- [ ] **Step 1: 테스트 작성**
```rust
#[tokio::test] async fn spec_profile_selects_connection_and_records_success() { 프로필(cliproxy, FakeProvider) + spec profile → 생성 OK, status.json ok:true, 결과 connection_source "profile:<name>" }
#[tokio::test] async fn quota_failure_is_recorded_as_last_generation() {}
#[tokio::test] async fn kind_model_mismatch_fails_before_call() { gemini 프로필 + gpt-image-2.5 → error_category unsupported, 요청 0건 }
#[tokio::test] async fn official_profile_defaults_to_four_workers() { openai 프로필 count 6 → concurrency 4, 동시 최대 4 }
#[tokio::test] async fn check_reports_each_stage() {
    // 닫힌 포트 → reachable false; 401 → reachable true, authenticated false; 200 → image_models 필터; 공용 http → warning Some
}
#[tokio::test] async fn check_never_generates() { 요청 경로가 모두 GET /models 계열 }
```
- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** PASS. **Step 5:** 커밋 `feat(core): run with profiles and report connection stages`.

### Task 6: CLI — `--profile`, `profile` 하위 명령

**Files:** Modify `crates/image-cli/src/main.rs`, `crates/image-core/src/schema.rs`(profile 설명 갱신). Test `crates/image-cli/tests/profile_cli.rs`.

명령(모든 출력은 JSON 한 줄, 실패 시 종료 코드 1):
```
image-cli profile list                         → {"ok":true,"default":id|null,"profiles":[{id,name,kind,base_url,concurrency,key_configured,warning}]}
image-cli profile add NAME --kind K [--url U] [--concurrency N] [--key-stdin | --key-env VAR] [--default]
image-cli profile update NAME [--name N] [--url U] [--concurrency N|none] [--key-stdin | --key-env VAR]
image-cli profile remove NAME                  → 키도 삭제
image-cli profile default NAME|--none
image-cli profile check [NAME]                 → ConnectionReport (NAME 없으면 해석 순서대로)
image-cli --profile NAME --list-models | --spec … | --json … | (stdin)
```
키는 명령줄 인자로 받지 않는다(셸 기록에 남음). `--key-stdin`은 표준입력 첫 줄.

- [ ] **Step 1: 테스트 작성** (`IMAGE_STUDIO_HOME`=임시 폴더, `IMAGE_STUDIO_KEYRING_SERVICE`=`ImageStudioTest-<uuid>`, 끝에서 `profile remove`로 키 정리; Windows 전용 `#[cfg(windows)]`)
```rust
#[test] fn add_with_key_from_stdin_then_list_shows_key_configured_without_key() { stdout·profiles.json 어디에도 키 문자열 없음 }
#[test] fn profile_flag_drives_generation_and_list_models() { FakeProvider cliproxy 프로필 → --profile로 생성 OK, CLIPROXY_BASE_URL을 다른 닫힌 포트로 줘도 프로필이 이김 }
#[test] fn check_outputs_report() {}
#[test] fn key_argument_is_refused() { "profile add x --kind openai --key sk-1" → 종료 코드 2, 사용법 오류 }
#[test] fn remove_deletes_key() { remove 후 같은 id의 키 조회 None (KeyringStore::with_service로 확인) }
```
- [ ] **Step 2:** FAIL. **Step 3:** 구현. **Step 4:** `cargo test --workspace` PASS. **Step 5:** 커밋 `feat(cli): manage connection profiles and select them with --profile`.

### Task 7: 문서 + 실검증

- [ ] **Step 1:** `docs/RUST_PARITY.md`에 profile 지원·종류별 동시성 기본값 추가. README Rust CLI 절에 프로필 사용 예(로컬 프록시, 노트북에서 Tailscale 주소, OpenAI 키). `INSTALL.md`에 "Rust CLI 연결 설정" 짧은 절. spec §6.3 기능표에 "문서 근거(2026-10-18) / 실측" 열 반영(OpenAI 공식에 plain `gpt-image-2.5` ID 없음, xAI 편집 JSON·최대 5장, Gemini `x-goog-api-key`·페이지).
- [ ] **Step 2 (실제 호출 1장):** `image-cli profile add local --kind cliproxy --url http://127.0.0.1:8317 --key-env CLIPROXY_API_KEY`, `image-cli profile check local`(ok 확인), `--profile local`로 `gpt-image-2.5` 1024² low 1장 dry-run 후 실행, 결과 이미지 확인. 이 `local` 프로필과 키는 남겨 둔다(사용자 PC 설정). 공식 API 프로필은 키가 없어 미검증으로 기록.
- [ ] **Step 3:** 커밋 `docs: document connection profiles and record stage 2 verification`.
