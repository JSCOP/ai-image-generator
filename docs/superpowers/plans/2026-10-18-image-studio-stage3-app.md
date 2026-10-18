# Image Studio 단계 3: Tauri 데스크톱 앱 1차 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 사람이 쓰는 Windows 데스크톱 앱 "Image Studio" 1차판. 연결 설정(프로필), 생성(병렬 진행·취소·이어하기·참조·모델 자체 투명), 갤러리(등록 폴더)를 다크 작업 도구형 화면으로 제공한다. 엔진은 `image-core`를 그대로 쓴다.

**Architecture:** `apps/studio/`에 Svelte 5 + TypeScript + Vite 화면, `apps/studio/src-tauri/`에 Tauri 2 크레이트 `studio`(workspace member). Tauri 크레이트는 얇다: `service` 모듈(순수 Rust, 테스트 가능)이 `image-core`를 부르고, `commands` 모듈은 이를 Tauri 명령으로 노출한다. 진행 상황은 `tauri::ipc::Channel`로 보낸다. 로컬 이미지는 asset 프로토콜(`convertFileSrc`)로 보여 주고, 등록 폴더는 실행 중 `asset_protocol_scope().allow_directory`로 허용한다.

**Tech Stack (2026-10-18 조사, `docs/superpowers/research/2026-10-18-provider-keyring-tauri-facts.md`):** tauri 2.12(features `protocol-asset`), tauri-build 2.7, tauri-plugin-dialog 2.8, tauri-plugin-opener 2.7, `@tauri-apps/api` ^2.12.4, `@tauri-apps/cli` ^2.12.3, `@tauri-apps/plugin-dialog` ^2.8.3, `@tauri-apps/plugin-opener` ^2.7.2, svelte ^5.57, `@sveltejs/vite-plugin-svelte` ^7.3, vite ^8.3, typescript **~6.0.3**(7은 svelte-check와 충돌), svelte-check ^4.7, `@tsconfig/svelte` ^5, vitest(최신 3.x 또는 vite 8과 맞는 버전), `dirs` 6, `chrono` 0.4. 패키지 관리자 pnpm(설치됨, store `E:\.pnpm-store`).

**Spec:** `docs/superpowers/specs/2026-10-18-image-studio-rust-app-design.md` §7.1, §7.4, §7.5, §8(화면), §10 단계 3. 단계 2 계획의 공용 인터페이스(`profiles`, `secrets`, `check`, `adapter`, `connection::resolve`)를 소비한다 — 실제 이름은 단계 2 완료 코드와 원장(`.superpowers/sdd/2026-10-18-image-studio-stage2-profiles/progress.md`)의 Ruling을 따른다.

## Global Constraints

- 단계 1·2 Global Constraints 유지. 키는 화면에서 입력받아 `SecretStore`로만 보내고, 어떤 명령도 키 값을 돌려주지 않는다(`key_configured: bool`만).
- 화면 글자는 한국어. 디자인은 다크 작업 도구형(시안 `ImageGallery/output/studio-redesign-concepts/01_proDark_generate.png` 참고): 배경 `#16181d`, 패널 `#1d2027`, 테두리 `#2a2e37`, 본문 `#e6e8ee`, 보조 `#9aa0ad`, 강조 보라 `#8b5cf6`(hover `#7c4dff`), 성공 `#22c55e`, 경고 `#f59e0b`, 오류 `#ef4444`. 글꼴 `"Segoe UI", "Malgun Gothic", system-ui`. 모서리 8px, 간격 8px 배수.
- 레이아웃: 왼쪽 아이콘 내비(생성·갤러리·연결), 생성 화면은 왼쪽 설정 패널 340px / 가운데 결과 / 오른쪽 기록 300px. 창 최소 1100×700.
- 노드 편집기 없음. 마스크 편집·배경 제거 화면은 단계 4(이번 범위 아님) — 메뉴에 넣지 않는다.
- 앱 홈: 단계 2의 `ProfileStore::default_dir`(= `IMAGE_STUDIO_HOME` 또는 `%APPDATA%\ImageStudio`). 앱 설정 `settings.json`도 같은 폴더.
- 기본 작업 폴더 `%USERPROFILE%\Pictures\Image Studio`(`dirs::picture_dir()/Image Studio`). 생성 결과는 `<작업 폴더>/ImageGallery/output/<topic>/`.
- topic 자동 이름: 프롬프트 첫 의미 있는 단어 최대 4개를 `safe_topic_slug`한 것 + `-YYYYMMDD-HHMMSS`(로컬 시각). 사용자가 "이어서 생성"을 고르면 원래 topic + `resume:true`.
- 생성 장수 1..=50(`tools/image_core.py` LIMITS.max_count), 참조 최대 4장·장당 10MB(초과는 화면에서 막고 서비스에서도 거부).
- 브라우저 `alert/confirm/prompt` 사용 금지(앱 안 대화상자 컴포넌트 사용).
- 앱 identifier `io.github.jscop.imagestudio`, productName `Image Studio`. 번들 대상 `nsis`(설치 파일 자체 검증은 단계 5).
- 실제 제공자 호출은 Task 9의 1장뿐.

## Review Focus

1. 생성 중 창을 닫거나 다른 화면으로 이동: 진행 중 실행은 계속되거나(이동) 취소되며(닫기 → `CancellationToken` 취소), lock이 남지 않는다. 다시 열면 갤러리에서 실패/완료가 구분된다. 테스트: Task 3(서비스), Task 9(수동 확인 목록).
2. 등록 폴더가 사라졌거나(외장 드라이브 분리) 권한이 없는 경우: 갤러리는 그 폴더를 "찾을 수 없음"으로 표시하고 나머지는 정상 표시. 테스트: Task 4.
3. 기록은 있는데 이미지가 없는 항목·Python이 만든 옛 기록(status 없음)·실패 기록: 각각 "파일 없음"·"완료"·"실패(원인)"로 구분 표시. 테스트: Task 4.
4. 아주 긴 한국어 프롬프트·줄바꿈·이모지: topic 이름이 안전하고, 화면 카드에서 말줄임 처리. 테스트: Task 3(topic), Task 5(포맷터).
5. 연결이 하나도 없는 첫 실행: 생성 버튼 대신 "연결을 먼저 추가하세요" 안내와 연결 화면 이동. 테스트: Task 7(vitest 상태), Task 9(목업 스크린샷).

---

## File Structure

```
Cargo.toml                                  # members += "apps/studio/src-tauri"
apps/studio/
  package.json, pnpm-lock.yaml, vite.config.ts, tsconfig.json, svelte.config.js, index.html
  src/main.ts, src/App.svelte
  src/lib/types.ts        # Rust 직렬화 타입의 TS 정의
  src/lib/api.ts          # invoke 래퍼 (명령 이름·인자 한곳)
  src/lib/state.svelte.ts # 앱 상태 (runes)
  src/lib/run.ts          # RunEvent → 카드 상태 reducer (순수 함수)
  src/lib/format.ts       # 크기·시간·용량·말줄임 포맷터 (순수 함수)
  src/lib/theme.css       # 디자인 토큰
  src/components/*.svelte # Button, Select, Segmented, Stepper, Dialog, Toast, Badge, StageBadges, ImageCard, Viewer, ReferencePicker, Empty
  src/screens/Generate.svelte, Gallery.svelte, Connections.svelte
  src/dev/mock-ipc.ts     # 개발 서버에서 ?mock=1일 때만 쓰는 가짜 명령 응답 (운영 빌드에 포함되지 않음)
  src/**/*.test.ts        # vitest
  scripts/make-icon.py    # 1024 PNG 아이콘 생성(PIL) → pnpm tauri icon
  src-tauri/Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/*
  src-tauri/src/main.rs, lib.rs
  src-tauri/src/service/{mod.rs, settings.rs, profiles.rs, generate.rs, gallery.rs}  # tauri 비의존, 단위 테스트
  src-tauri/src/commands.rs   # #[tauri::command] 래퍼 + 오류 직렬화
  src-tauri/src/state.rs      # AppState
  src-tauri/tests/*.rs        # 서비스 통합 테스트 (FakeProvider)
```

## 공용 인터페이스 (Rust → TS 동일 이름, serde camelCase)

```rust
// service/settings.rs
#[derive(Serialize, Deserialize, Clone)] #[serde(rename_all = "camelCase")]
pub struct Settings { pub work_dir: PathBuf, pub gallery_folders: Vec<PathBuf>, pub last_profile: Option<String>, pub last_model: Option<String> }
pub fn load_settings(home: &Path) -> Settings;            // 없거나 깨졌으면 기본값 (깨진 파일은 settings.json.bad-<시각>으로 옮김)
pub fn save_settings(home: &Path, s: &Settings) -> Result<(), ServiceError>;

// service/mod.rs
#[derive(Serialize, Debug)] #[serde(rename_all = "camelCase")]
pub struct ServiceError { pub category: Category, pub message: String }   // EngineError에서 변환, 키 없음
pub struct Ctx { pub home: PathBuf, pub env: Arc<dyn EnvSource>, pub secrets: Arc<dyn SecretStore> }

// service/profiles.rs
#[serde(rename_all = "camelCase")] pub struct ProfileView { pub id, pub name, pub kind: ProviderKind, pub kind_label: String, pub base_url,
    pub concurrency: Option<u32>, pub is_default: bool, pub key_configured: bool, pub warning: Option<String>, pub last_generation: Option<GenerationStatus> }
#[serde(rename_all = "camelCase")] pub struct EnvConnectionView { pub base_url: String, pub source: String, pub key_configured: bool }
#[serde(rename_all = "camelCase")] pub struct ConnectionsView { pub profiles: Vec<ProfileView>, pub env: Option<EnvConnectionView>, pub active: Option<String> /* 해석 순서상 지금 쓰일 연결의 이름 또는 "env" */ }
#[serde(rename_all = "camelCase")] pub struct ProfileInput { pub id: Option<String>, pub name: String, pub kind: ProviderKind, pub base_url: Option<String>,
    pub concurrency: Option<u32>, pub key: Option<String> /* None=변경 없음, Some("")=키 삭제 */, pub make_default: bool }
pub fn connections(ctx: &Ctx) -> Result<ConnectionsView, ServiceError>;
pub fn save_profile(ctx: &Ctx, input: ProfileInput) -> Result<ProfileView, ServiceError>;
pub fn remove_profile(ctx: &Ctx, id: &str) -> Result<(), ServiceError>;
pub fn set_default_profile(ctx: &Ctx, id: Option<&str>) -> Result<(), ServiceError>;
pub async fn check_profile(ctx: &Ctx, name: Option<&str>) -> Result<ConnectionReport, ServiceError>;
#[serde(rename_all = "camelCase")] pub struct ModelView { pub id: String, pub provider: String, pub references: bool, pub transparency: bool }
pub async fn models(ctx: &Ctx, name: Option<&str>) -> Result<Vec<ModelView>, ServiceError>;

// service/generate.rs
#[serde(rename_all = "camelCase")] pub struct GenerateInput { pub profile: Option<String>, pub prompt: String, pub avoid: String, pub image_model: String,
    pub size: String, pub quality: String, pub count: u32, pub references: Vec<PathBuf>, pub background: String /* auto|transparent */,
    pub topic: Option<String>, pub resume: bool }
#[serde(tag = "event", content = "data", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RunEvent { Started { run_id: String, total: u32, output_dir: String, topic: String },
    ImageStarted { index: u32, output: String }, ImageRetry { index: u32, attempt: u32, delay_sec: f64, reason: String },
    ImageFinished { index: u32, output: String, elapsed_sec: f64 }, ImageSkipped { index: u32, output: String },
    ImageFailed { index: u32, reason: String }, Finished { summary: RunSummary } }
#[serde(rename_all = "camelCase")] pub struct RunSummary { pub ok: bool, pub outputs: Vec<String>, pub failures: Vec<FailureView>, pub topic: String,
    pub output_dir: String, pub elapsed_sec: f64, pub cancelled: bool, pub warnings: Vec<String> }
#[serde(rename_all = "camelCase")] pub struct FailureView { pub index: u32, pub reason: String, pub category: Option<Category>, pub provider_code: Option<String> }
pub fn build_spec(input: &GenerateInput, settings: &Settings, now: chrono::DateTime<chrono::Local>) -> Result<(serde_json::Value, String /*topic*/), ServiceError>;
pub async fn run_generation(ctx: &Ctx, input: GenerateInput, emit: impl Fn(RunEvent) + Send + Sync + 'static, cancel: CancellationToken) -> Result<RunSummary, ServiceError>;

// service/gallery.rs
#[serde(rename_all = "camelCase")] pub struct GalleryItem { pub image_path: String, pub record_path: String, pub folder: String, pub topic: String,
    pub status: String /* complete|pending|failed|legacy|missing */, pub prompt: String, pub image_model: String, pub response_model: Option<String>,
    pub size: String, pub background: Option<String>, pub transparent_ratio: Option<f64>, pub error: Option<String>, pub modified: f64 }
#[serde(rename_all = "camelCase")] pub struct GalleryView { pub items: Vec<GalleryItem>, pub folders: Vec<FolderView>, pub truncated: bool }
#[serde(rename_all = "camelCase")] pub struct FolderView { pub path: String, pub gallery_root: String, pub available: bool, pub is_work_dir: bool }
pub fn gallery(settings: &Settings, limit: usize) -> GalleryView;   // 최신순, 기본 limit 2000
pub fn normalize_folders(folders: &[PathBuf]) -> Vec<PathBuf>;     // gallery_root 기준 정규화·중복 제거(대소문자 무시)
```

Tauri 명령(`commands.rs`, 이름 = TS `api.ts` 이름): `app_info`, `get_settings`, `save_settings`, `connections`, `save_profile`, `remove_profile`, `set_default_profile`, `check_profile`, `list_models`, `start_generation(input, on_event: Channel<RunEvent>) -> RunSummary`, `cancel_generation(run_id)`, `gallery(limit)`, `add_gallery_folder(path)`, `remove_gallery_folder(path)`. 모든 명령의 오류 형은 `ServiceError`.

---

### Task 1: 앱 뼈대 (빈 창이 뜬다)

**Files:** Create everything under `apps/studio/` listed above except screens/components detail; Modify root `Cargo.toml`, `.gitignore`(`apps/studio/node_modules/`, `apps/studio/dist/`, `apps/studio/src-tauri/gen/`).

- [ ] **Step 1:** `apps/studio/scripts/make-icon.py`(Pillow, 저장소 `.venv` 사용): 1024×1024 PNG, 배경 보라 그라데이션 둥근 사각형(`#7c4dff`→`#8b5cf6`), 가운데 흰색 굵은 사각 프레임 + 작은 반짝임(별) 도형. 실행 후 `pnpm tauri icon <png>`로 `src-tauri/icons/` 생성.
- [ ] **Step 2: 실패하는 테스트** — `src-tauri/src/lib.rs`의 `#[cfg(test)]`에 `tauri::test::mock_builder`로 `app_info` 호출 → `{"version":"0.1.0","home":…,"workDir":…}` 확인(연구 노트의 headless 테스트 예시 형식). vitest: `src/lib/format.test.ts`에 `formatSize("1920x1080") === "1920×1080"` 하나.
- [ ] **Step 3:** 뼈대 구현: Vite 설정(포트 1420 strictPort, `build.target` chrome105, `minify: !process.env.TAURI_ENV_DEBUG`), tauri.conf.json(`frontendDist ../dist`, `devUrl http://localhost:1420`, `beforeDevCommand "pnpm dev"`, `beforeBuildCommand "pnpm build"`, 창 label `main` 1440×900 min 1100×700 제목 "Image Studio", `security.csp`는 `default-src 'self'; img-src 'self' asset: http://asset.localhost blob: data:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost`, `assetProtocol.enable true, scope []`, bundle targets `["nsis"]`, webviewInstallMode downloadBootstrapper), capabilities(core:default, dialog:default, opener:default). `[profile.release]`은 루트에 두지 않는다(image-cli에 panic=abort를 강요하지 않음). App.svelte는 다크 배경에 "Image Studio" 제목만.
- [ ] **Step 4:** `pnpm install`(apps/studio), `pnpm build`, `pnpm check`(svelte-check 0 오류), `pnpm test`, `cargo test -p studio`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm tauri build --debug --no-bundle` 성공. 만들어진 `target/debug/studio.exe`를 백그라운드로 실행해 5초 뒤 프로세스가 살아 있는지 확인하고 종료.
- [ ] **Step 5:** 커밋 `feat(studio): scaffold Tauri 2 + Svelte app`.

### Task 2: 설정·연결 서비스와 명령

**Files:** `service/{mod.rs,settings.rs,profiles.rs}`, `commands.rs`, `state.rs`. Test `src-tauri/tests/service_profiles.rs`(tempdir home, `MemoryStore`, `MapEnv`, `FakeProvider`).

- [ ] **Step 1: 테스트 작성**
```rust
#[test] fn settings_default_work_dir_is_pictures_image_studio() {}
#[test] fn corrupt_settings_are_moved_aside_and_defaults_used() {}
#[test] fn save_profile_stores_key_only_in_secret_store() { profiles.json·반환값 직렬화 어디에도 키 없음, key_configured true }
#[test] fn save_profile_with_empty_key_deletes_key() {}
#[test] fn connections_view_shows_env_and_active() { env 있음 → env Some, active "env"; env 없고 기본 프로필 → active=프로필 이름 }
#[tokio::test] async fn models_view_marks_capabilities() { FakeProvider /models → transparency는 sunburst만, grok references false(cliproxy) }
#[tokio::test] async fn check_profile_returns_stage_report() {}
#[test] fn service_error_never_contains_key() { 403 본문에 키를 돌려주는 제공자 → ServiceError.message에 키 없음 }
```
- [ ] **Step 2–4:** FAIL → 구현 → `cargo test -p studio` PASS. **Step 5:** 커밋 `feat(studio): settings and connection profile commands`.

### Task 3: 생성 서비스 — 진행 이벤트·취소·이어하기

**Files:** `service/generate.rs`, `commands.rs`(start/cancel), `state.rs`(실행 중 토큰 `Mutex<HashMap<String, CancellationToken>>`, 창 `CloseRequested` 시 전부 취소). Test `src-tauri/tests/service_generate.rs`.

`build_spec`은 `{mode:"single", prompt: compose(prompt, avoid), topic, topic_root: work_dir, count, size, quality, image_model, reference_images, background, resume, profile?}`를 만든다. `compose`는 Python `compose_prompt`와 같다(`prompt` + 줄바꿈 + `Avoid: …`). 실행은 `image_core::runner::run_spec_with`에 `RunOptions{profiles: Some(store), secrets, cwd: work_dir, …}`를 넘기고, 엔진 진행 JSON(`image_started|image_finished|image_failed|image_skipped|retry`)을 `RunEvent`로 바꾼다. 마지막에 결과 JSON을 `RunSummary`로(실패의 `error.category/provider_code` 포함, `cancelled`는 취소 토큰 상태).

- [ ] **Step 1: 테스트 작성**
```rust
#[test] fn build_spec_composes_avoid_and_topic() { now 2026-10-18 14:05:09, prompt "아늑한 독서 공간, 오후 햇살 ☀️" → topic "아늑한_독서_공간_오후_햇살-20261018-140509" 이하 80자, prompt 두 줄째 "Avoid: text" }
#[test] fn build_spec_validates_count_and_references() { count 0/51 → invalid_request; 참조 5장 → invalid_request; 11MB 참조 → invalid_request }
#[test] fn resume_keeps_topic() {}
#[tokio::test] async fn run_emits_events_in_order_and_summary() { FakeProvider, count 3 → Started → ImageStarted×3 → ImageFinished×3 → Finished(ok, outputs 3) }
#[tokio::test] async fn failure_carries_category() { 403 spending-limit → Finished.summary.failures[0].category == Quota, provider_code 포함 }
#[tokio::test] async fn cancel_marks_summary_cancelled_and_leaves_no_lock() { 지연 응답, 100ms 후 cancel → cancelled true, metadata에 .lock 없음 }
#[tokio::test] async fn transparent_on_unsupported_model_fails_before_call() {}
```
- [ ] **Step 2–4:** FAIL → 구현 → PASS. **Step 5:** 커밋 `feat(studio): generation service with progress channel, cancel and resume`.

### Task 4: 갤러리 서비스

**Files:** `service/gallery.rs`, `commands.rs`(gallery, add/remove folder — 추가·시작 시 `app.asset_protocol_scope().allow_directory(<gallery_root>, true)`), Test `src-tauri/tests/service_gallery.rs`.

- [ ] **Step 1: 테스트 작성**
```rust
#[test] fn lists_complete_failed_pending_legacy_and_missing() {
    // metadata/t/a.png.json status complete + 이미지 → complete; failed + error → failed(error 메시지); 
    // pending + 이미지 없음 → pending; status 없는 Python 기록 + 이미지 → legacy; complete인데 이미지 없음 → missing
}
#[test] fn newest_first_and_limit_sets_truncated() {}
#[test] fn skips_lock_tmp_and_non_json() {}
#[test] fn unavailable_folder_is_reported_not_fatal() { 없는 폴더 → FolderView.available false, 다른 폴더 항목은 나옴 }
#[test] fn normalize_folders_dedupes_case_insensitively_and_maps_to_gallery_root() { "X\\ImageGallery\\output" 와 "x" → 하나 }
#[test] fn corrupt_record_is_skipped_not_fatal() {}
```
- [ ] **Step 2–4:** FAIL → 구현(폴더별 `ImageGallery/metadata/*/` 한 단계 아래 `*.json`만 읽음, 하위 폴더 재귀 1단계 허용) → PASS. **Step 5:** 커밋 `feat(studio): gallery over registered folders`.

### Task 5: 화면 기반 — 디자인 토큰, 셸, API 계층, 개발용 목업

**Files:** `src/lib/{types.ts, api.ts, state.svelte.ts, run.ts, format.ts, theme.css}`, `src/components/*`, `src/App.svelte`, `src/dev/mock-ipc.ts`, tests `src/lib/*.test.ts`.

- `api.ts`: 위 명령마다 타입 있는 함수. `startGeneration(input, onEvent)`은 `new Channel<RunEvent>()`.
- `run.ts`: `reduce(cards: Card[], ev: RunEvent): Card[]` — 카드 상태 `waiting|running|retrying|done|skipped|failed`, 재시도 원인 표시, 카운터 `{done, running, retrying, failed, total}`.
- `format.ts`: `formatSize`, `formatRelativeTime(epochSec, now)`(오늘 14:05 / 어제 09:10 / 10월 3일), `truncateMiddle`, `categoryLabel(Category)` 한국어(`network:"연결 끊김"`, `dns:"주소를 찾을 수 없음(오타 또는 Tailscale 꺼짐)"`, `timeout:"시간 초과"`, `tls:"인증서 문제"`, `auth:"키가 거부됨"`, `permission:"권한 없음"`, `quota:"사용 한도 초과"`, `rate_limit:"요청이 너무 많음(잠시 후 재시도)"`, `conflict:"다른 작업과 겹침"`, `cancelled:"취소됨"`, `not_found:"주소 경로 오류(/v1 확인)"`, `redirect:"다른 주소로 이동 요구(따라가지 않음)"`, `unsupported:"이 연결/모델에서 지원하지 않음"`, `invalid_request:"요청 오류"`, `provider:"제공자 오류"`, `internal:"앱 내부 오류"`).
- 셸: 왼쪽 64px 아이콘 내비(생성/갤러리/연결, 아이콘은 인라인 SVG), 상단 바(앱 이름, 현재 연결 배지: 이름·종류·4단계 상태 점), 토스트.
- `dev/mock-ipc.ts`: `import.meta.env.DEV`이고 URL에 `?mock=1`이면 `@tauri-apps/api/mocks`의 `mockIPC`로 모든 명령에 고정 응답(프로필 2개, 모델 6개, 갤러리 12장 — 이미지 경로는 `public/mock/*.png` 4장을 저장소의 기존 생성 결과에서 256px로 줄여 복사), `start_generation`은 타이머로 이벤트를 흘려보냄. 운영 빌드에서 제외됨을 `pnpm build` 산출물 grep으로 확인.
- [ ] **Step 1: vitest 작성** — `run.test.ts`(이벤트 순서별 카드 상태·카운터, 실패 카테고리 라벨), `format.test.ts`(상대 시간 경계, 긴 한국어+이모지 말줄임이 서로게이트를 자르지 않음).
- [ ] **Step 2–4:** FAIL → 구현 → `pnpm test`, `pnpm check` PASS. **Step 5:** 커밋 `feat(studio): design tokens, app shell, typed API and dev mock`.

### Task 6: 연결 화면

**Files:** `src/screens/Connections.svelte`, 필요한 컴포넌트, test `src/screens/connections.test.ts`(상태 로직만 분리한 `connectionsForm.ts`의 단위 테스트).

화면: 왼쪽 프로필 목록(이름, 종류 라벨, 주소, 기본 표시, 키 상태, 4단계 점: 서버·인증·모델·마지막 생성), "환경변수 연결" 카드(읽기 전용, 활성 여부), 오른쪽 편집 폼(종류 선택 시 기본 주소 자동 입력·공식 종류는 주소 잠금 해제 가능, 키 입력은 password 필드 + "저장된 키 유지/바꾸기/지우기", 동시 실행 수, 기본으로 지정), "연결 확인" 버튼 → 단계별 결과와 오류 원인 한국어, 공용 http 경고 노란 띠. 삭제는 앱 안 확인 대화상자.
- [ ] **Step 1: 테스트** — `connectionsForm.ts`: 종류 바꿀 때 주소 자동 채움 규칙, 이름 공백 검증, 키 상태 3가지에 따른 `ProfileInput.key` 값(None/Some(새 키)/Some("")).
- [ ] **Step 2–4:** FAIL → 구현 → PASS, `pnpm check` 0 오류. **Step 5:** 커밋 `feat(studio): connections screen`.

### Task 7: 생성 화면

**Files:** `src/screens/Generate.svelte`, `src/components/{ReferencePicker, ImageCard, Viewer}.svelte`, `src/lib/generateForm.ts` + test.

왼쪽 패널: 프롬프트(글자 수), Avoid(접기), 연결 선택(기본=활성 연결), 모델(연결 확인된 목록, 기능 배지: 참조·투명), 크기 칩(1920×1080, 1024×1024, 1080×1920, 1536×1024, 1024×1536, 2048×2048, 3840×2160, 직접 입력), 품질 세그먼트(낮음/보통/높음), 장수 스테퍼(1–50), 참조 이미지(파일 선택 대화상자 + 창에 끌어놓기 `getCurrentWebview().onDragDropEvent`, 썸네일·삭제, 모델이 참조 불가면 비활성+이유), 배경(자동 / 투명 — 모델이 투명 지원일 때만, 아니면 비활성+"이 모델은 투명 배경을 지원하지 않습니다"), "N장 생성" 버튼(실행 중에는 "취소"). 가운데: 상단 진행 막대와 카운터(완료·진행·재시도·실패), 카드 그리드(대기 스켈레톤, 진행 스피너, 재시도 원인 배지, 실패 원인, 완료 이미지 — 투명 이미지는 체크무늬 배경), 카드 클릭 → 큰 보기(Viewer: 확대, 공급자 원본 크기→저장 크기, 사용 모델(요청 ID), 탐색기에서 보기). 오른쪽: 기록(갤러리에서 최근 topic별 묶음, 클릭 시 결과 다시 보기, "이 설정 다시 쓰기", "이어서 생성").
연결이 없으면 가운데에 "연결을 먼저 추가하세요" + 연결 화면 버튼.
- [ ] **Step 1: 테스트** — `generateForm.ts`: 입력 → `GenerateInput` 변환, 투명/참조 가능 여부 판정, 장수·참조 한도, 크기 직접 입력 검증, 연결 없음 상태.
- [ ] **Step 2–4:** FAIL → 구현 → PASS. **Step 5:** 커밋 `feat(studio): generate screen with live progress`.

### Task 8: 갤러리 화면

**Files:** `src/screens/Gallery.svelte`, `src/lib/galleryView.ts` + test.

상단: 폴더 필터(전체/폴더별, 사용 불가 폴더는 회색+"찾을 수 없음"), 폴더 추가(폴더 선택 대화상자)·제거, 상태 필터(전체/완료/실패/파일 없음), 검색(프롬프트·모델). 본문: 날짜별 묶음 그리드, 카드에 상태 배지·모델·크기, 실패 카드는 원인. 선택 시 오른쪽 상세(프롬프트 전체, 기록 JSON의 주요 값, 버튼: 탐색기에서 보기, 이 설정 다시 쓰기, 참조로 사용, 이어서 생성).
- [ ] **Step 1: 테스트** — `galleryView.ts`: 필터 조합, 날짜 묶음(오늘/어제/날짜), 검색 대소문자 무시.
- [ ] **Step 2–4:** FAIL → 구현 → PASS. **Step 5:** 커밋 `feat(studio): gallery screen`.

### Task 9: 화면 검증 + 실제 실행 + 문서

- [ ] **Step 1 (목업 시각 검증):** `pnpm dev`를 백그라운드로 띄우고 Playwright(MCP 브라우저 도구)로 `http://localhost:1420/?mock=1`의 생성(진행 중·완료)·갤러리·연결(확인 결과 포함)·첫 실행(연결 없음) 화면을 1440×900과 1100×700에서 캡처해 `apps/studio/docs/screenshots/`에 저장하고 직접 본다. 콘솔 오류 0건. 겹침·잘림·영어 잔여 문구·대비 부족을 고친다.
- [ ] **Step 2 (실제 앱 실행):** `pnpm tauri build --debug --no-bundle` 후 `target/debug/studio.exe`를 실행해 창이 뜨고 10초간 살아 있는지, 앱 홈에 settings.json이 생기는지 확인 후 종료.
- [ ] **Step 3 (실제 1장, 서비스 경로):** `src-tauri/tests/real_proxy.rs`에 `#[ignore]` 테스트 `real_proxy_one_image` — 사용자 프로필 `local`(단계 2에서 만든 것)로 `run_generation` 1장(`gpt-image-2.5`, 1024×1024, low, 작업 폴더는 임시 폴더가 아니라 `E:/ai-image-generator`의 `ImageGallery/output/studio-app-smoke-20261018`). `cargo test -p studio --test real_proxy -- --ignored` 실행, 이미지 확인.
- [ ] **Step 4 (문서):** README에 "Image Studio 앱" 절(개발 실행 `pnpm --dir apps/studio tauri dev`, 디버그 빌드, 화면 설명, 스크린샷 링크), spec §10 단계 3 칸에 결과·날짜, SSOT 한 줄. 사용자 수동 확인 목록(앱에서 실제 생성·취소·이어하기, 창 닫기 중 취소)을 README에 적는다.
- [ ] **Step 5:** 커밋 `docs: record stage 3 app verification`.
