use image_core::error::Category;
use image_core::profiles::{transport_warning, GenerationStatus, ProfilePatch, ProfileStore, ProviderKind};
use std::path::PathBuf;
use test_fixture::MapEnv;

fn temp_store() -> (tempfile::TempDir, ProfileStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ProfileStore::open(dir.path().to_path_buf());
    (dir, store)
}

#[test]
fn add_list_default_remove() {
    let (_dir, store) = temp_store();
    assert!(store.load().unwrap().profiles.is_empty());
    let p = store.add("집 PC", ProviderKind::Cliproxy, Some("http://100.98.54.122:8317"), None).unwrap();
    assert!(p.id.starts_with("p-") && p.id.len() == 10, "{}", p.id);
    assert!(p.id[2..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!(p.base_url, "http://100.98.54.122:8317/v1");
    assert_eq!(p.name, "집 PC");
    assert_eq!(store.load().unwrap().profiles, vec![p.clone()]);
    store.set_default(Some("집 PC")).unwrap();
    assert_eq!(store.load().unwrap().default, Some(p.id.clone()));
    assert_eq!(store.find("집 pc").unwrap(), p);
    assert_eq!(store.find(&p.id).unwrap(), p);
    store.remove(&p.id).unwrap();
    let file = store.load().unwrap();
    assert_eq!(file.default, None);
    assert!(file.profiles.is_empty());
    assert_eq!(store.find(&p.id).unwrap_err().category, Category::NotFound);
    assert_eq!(store.remove("집 PC").unwrap_err().category, Category::NotFound);
}

#[test]
fn ids_are_distinct_for_similar_names() {
    let (_dir, store) = temp_store();
    let a = store.add("my.pc", ProviderKind::Cliproxy, Some("http://127.0.0.1:8317"), None).unwrap();
    let b = store.add("my pc", ProviderKind::Cliproxy, Some("http://127.0.0.1:8317"), None).unwrap();
    assert_ne!(a.id, b.id);
}

#[test]
fn set_default_none_clears_and_unknown_is_not_found() {
    let (_dir, store) = temp_store();
    let p = store.add("a", ProviderKind::Openai, None, None).unwrap();
    store.set_default(Some(&p.id)).unwrap();
    assert_eq!(store.set_default(Some("missing")).unwrap_err().category, Category::NotFound);
    assert_eq!(store.load().unwrap().default, Some(p.id));
    store.set_default(None).unwrap();
    assert_eq!(store.load().unwrap().default, None);
}

#[test]
fn official_kinds_fill_default_urls() {
    let (_dir, store) = temp_store();
    assert_eq!(store.add("o", ProviderKind::Openai, None, None).unwrap().base_url, "https://api.openai.com/v1");
    assert_eq!(store.add("g", ProviderKind::Gemini, None, None).unwrap().base_url, "https://generativelanguage.googleapis.com");
    assert_eq!(store.add("x", ProviderKind::Xai, None, None).unwrap().base_url, "https://api.x.ai/v1");
    // Gemini keeps its path as given (no /v1), only the trailing slash goes.
    let g = store.add("g2", ProviderKind::Gemini, Some("https://gw.example.com/gemini/"), None).unwrap();
    assert_eq!(g.base_url, "https://gw.example.com/gemini");
    let err = store.add("g3", ProviderKind::Gemini, Some("https://generativelanguage.googleapis.com?key=abc"), None).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(!err.message.contains("abc"));
}

#[test]
fn gemini_url_with_api_version_path_is_refused() {
    let (_dir, store) = temp_store();
    for raw in [
        "https://generativelanguage.googleapis.com/v1beta",
        "https://generativelanguage.googleapis.com/v1beta/",
        "https://generativelanguage.googleapis.com/V1BETA",
        "https://generativelanguage.googleapis.com/v1beta/models",
        "https://gw.example.com/gemini/v1beta",
    ] {
        let err = store.add("g", ProviderKind::Gemini, Some(raw), None).unwrap_err();
        assert_eq!(err.category, Category::InvalidRequest, "{raw}");
        assert!(err.message.contains("/v1beta") && err.message.contains("호스트"), "{}", err.message);
        assert_eq!(ProviderKind::Gemini.normalize_base_url(raw).unwrap_err().category, Category::InvalidRequest);
    }
    assert!(store.load().unwrap().profiles.is_empty());
    let g = store.add("g", ProviderKind::Gemini, None, None).unwrap();
    assert_eq!(
        store.update("g", ProfilePatch { base_url: Some(format!("{}/v1beta", g.base_url)), ..Default::default() }).unwrap_err().category,
        Category::InvalidRequest
    );
    // A path that merely contains the letters is fine.
    assert!(store.add("g2", ProviderKind::Gemini, Some("https://gw.example.com/v1betas"), None).is_ok());
}

fn write_profiles(dir: &tempfile::TempDir, value: serde_json::Value) -> PathBuf {
    let path = dir.path().join("profiles.json");
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    path
}

#[test]
fn hand_edited_bad_entries_are_reported_by_name() {
    let good = serde_json::json!({"id": "p-00000001", "name": "ok", "kind": "openai", "base_url": "https://api.openai.com/v1"});
    let cases = [
        (serde_json::json!({"id": "p-00000002", "name": "bad url", "kind": "cliproxy", "base_url": "http://u:pw@host:1/v1"}), "bad url"),
        (
            serde_json::json!({"id": "p-00000002", "name": "gem", "kind": "gemini", "base_url": "https://generativelanguage.googleapis.com/v1beta"}),
            "gem",
        ),
        (
            serde_json::json!({"id": "p-00000002", "name": "many", "kind": "openai", "base_url": "https://api.openai.com/v1", "concurrency": 0}),
            "many",
        ),
        (
            serde_json::json!({"id": "p-00000002", "name": "huge", "kind": "openai", "base_url": "https://api.openai.com/v1", "concurrency": 33}),
            "huge",
        ),
        (serde_json::json!({"id": "p-00000002", "name": "  ", "kind": "openai", "base_url": "https://api.openai.com/v1"}), "p-00000002"),
        (
            serde_json::json!({"id": "p-00000002", "name": "p-0000abcd", "kind": "openai", "base_url": "https://api.openai.com/v1"}),
            "p-0000abcd",
        ),
        (serde_json::json!({"id": "p-00000002", "name": " padded ", "kind": "openai", "base_url": "https://api.openai.com/v1"}), "padded"),
        (serde_json::json!({"id": "p-00000002", "name": "OK", "kind": "openai", "base_url": "https://api.openai.com/v1"}), "OK"),
        (serde_json::json!({"id": "p-00000001", "name": "twin", "kind": "openai", "base_url": "https://api.openai.com/v1"}), "p-00000001"),
    ];
    for (bad, label) in cases {
        let (dir, store) = temp_store();
        let path = write_profiles(&dir, serde_json::json!({"version": 1, "default": null, "profiles": [good, bad]}));
        let before = std::fs::read_to_string(&path).unwrap();
        let err = store.load().unwrap_err();
        assert_eq!(err.category, Category::InvalidRequest, "{bad}");
        assert!(err.message.contains("프로필 파일을 읽을 수 없") && err.message.contains(label), "{bad}: {}", err.message);
        assert!(store.add("new", ProviderKind::Openai, None, None).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "never overwritten");
    }
}

#[test]
fn default_naming_a_missing_profile_means_no_default() {
    let (dir, store) = temp_store();
    let entry = serde_json::json!({"id": "p-00000001", "name": "a", "kind": "openai", "base_url": "https://api.openai.com/v1"});
    write_profiles(&dir, serde_json::json!({"version": 1, "default": "p-deadbeef", "profiles": [entry]}));
    let file = store.load().unwrap();
    assert_eq!(file.default, None);
    assert_eq!(file.profiles.len(), 1);
    assert_eq!(store.find("a").unwrap().id, "p-00000001");
    // Edits keep working and write the repaired default.
    store.add("b", ProviderKind::Xai, None, None).unwrap();
    store.set_default(Some("a")).unwrap();
    assert_eq!(store.load().unwrap().default.as_deref(), Some("p-00000001"));
    write_profiles(&dir, serde_json::json!({"version": 1, "default": "p-deadbeef", "profiles": [entry]}));
    store.remove("a").unwrap();
    assert_eq!(store.load().unwrap(), image_core::profiles::ProfileFile { version: 1, default: None, profiles: vec![] });
}

#[test]
fn cliproxy_requires_url() {
    let (_dir, store) = temp_store();
    assert_eq!(store.add("x", ProviderKind::Cliproxy, None, None).unwrap_err().category, Category::InvalidRequest);
    assert_eq!(store.add("y", ProviderKind::OpenaiCompatible, None, None).unwrap_err().category, Category::InvalidRequest);
    assert_eq!(store.add("z", ProviderKind::Cliproxy, Some("http://u:p@host:1"), None).unwrap_err().category, Category::InvalidRequest);
    assert!(store.load().unwrap().profiles.is_empty());
}

#[test]
fn names_are_unique_case_insensitive_and_trimmed() {
    let (_dir, store) = temp_store();
    let p = store.add("  Home ", ProviderKind::Openai, None, None).unwrap();
    assert_eq!(p.name, "Home");
    assert_eq!(store.add(" home ", ProviderKind::Openai, None, None).unwrap_err().category, Category::Conflict);
    assert_eq!(store.add("   ", ProviderKind::Openai, None, None).unwrap_err().category, Category::InvalidRequest);
    assert_eq!(store.add("p-0123abcd", ProviderKind::Openai, None, None).unwrap_err().category, Category::InvalidRequest);
}

#[test]
fn concurrency_must_be_in_range() {
    let (_dir, store) = temp_store();
    assert_eq!(store.add("a", ProviderKind::Openai, None, Some(0)).unwrap_err().category, Category::InvalidRequest);
    assert_eq!(store.add("a", ProviderKind::Openai, None, Some(33)).unwrap_err().category, Category::InvalidRequest);
    assert_eq!(store.add("a", ProviderKind::Openai, None, Some(8)).unwrap().concurrency, Some(8));
}

#[test]
fn update_renames_and_changes_url_and_concurrency() {
    let (_dir, store) = temp_store();
    let a = store.add("a", ProviderKind::Cliproxy, Some("http://127.0.0.1:8317"), Some(10)).unwrap();
    store.add("b", ProviderKind::Openai, None, None).unwrap();
    let patch = ProfilePatch { name: Some(" A2 ".into()), base_url: Some("http://127.0.0.1:9000/v1/".into()), concurrency: Some(None) };
    let updated = store.update("A", patch).unwrap();
    assert_eq!(updated.id, a.id);
    assert_eq!(updated.name, "A2");
    assert_eq!(updated.base_url, "http://127.0.0.1:9000/v1");
    assert_eq!(updated.concurrency, None);
    assert_eq!(store.find("a2").unwrap(), updated);
    // Renaming only the case of its own name is fine; taking another profile's name is not.
    assert_eq!(store.update(&a.id, ProfilePatch { name: Some("a2".into()), ..Default::default() }).unwrap().name, "a2");
    let err = store.update(&a.id, ProfilePatch { name: Some("B".into()), ..Default::default() }).unwrap_err();
    assert_eq!(err.category, Category::Conflict);
    let changed = store.update(&a.id, ProfilePatch { concurrency: Some(Some(5)), ..Default::default() }).unwrap();
    assert_eq!(changed.concurrency, Some(5));
    assert_eq!(changed.name, "a2");
    assert_eq!(store.update("missing", ProfilePatch::default()).unwrap_err().category, Category::NotFound);
}

#[test]
fn corrupt_file_is_reported_and_left_untouched() {
    let (dir, store) = temp_store();
    let path = dir.path().join("profiles.json");
    std::fs::write(&path, "{not json").unwrap();
    let err = store.load().unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("프로필 파일을 읽을 수 없"), "{}", err.message);
    assert!(store.add("a", ProviderKind::Openai, None, None).is_err());
    assert!(store.set_default(None).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
}

#[test]
fn unknown_version_is_rejected() {
    let (dir, store) = temp_store();
    let path = dir.path().join("profiles.json");
    let text = r#"{"version": 9, "default": null, "profiles": []}"#;
    std::fs::write(&path, text).unwrap();
    let err = store.load().unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(err.message.contains("프로필 파일 버전 9"), "{}", err.message);
    assert!(store.add("a", ProviderKind::Openai, None, None).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn file_never_contains_keys() {
    let (dir, store) = temp_store();
    let a = store.add("a", ProviderKind::Openai, None, Some(4)).unwrap();
    let b = store.add("b", ProviderKind::OpenaiCompatible, Some("http://10.0.0.5:1"), None).unwrap();
    store.set_default(Some("a")).unwrap();
    let text = std::fs::read_to_string(dir.path().join("profiles.json")).unwrap();
    assert!(!text.to_lowercase().contains("key"), "{text}");
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let expected = serde_json::json!({
        "version": 1,
        "default": a.id,
        "profiles": [
            {"id": a.id, "name": "a", "kind": "openai", "base_url": "https://api.openai.com/v1", "concurrency": 4},
            {"id": b.id, "name": "b", "kind": "openai_compatible", "base_url": "http://10.0.0.5:1/v1"},
        ],
    });
    assert_eq!(value, expected);
    let names: Vec<String> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["profiles.json".to_string()], "no lock or temp files left behind");
}

#[test]
fn held_lock_times_out_as_conflict_without_writing() {
    let (dir, store) = temp_store();
    std::fs::write(dir.path().join("profiles.json.lock"), "").unwrap();
    let started = std::time::Instant::now();
    let err = store.add("a", ProviderKind::Openai, None, None).unwrap_err();
    assert_eq!(err.category, Category::Conflict);
    assert!(started.elapsed() >= std::time::Duration::from_secs(4));
    assert!(!dir.path().join("profiles.json").exists());
}

#[test]
fn provider_kind_strings_and_defaults() {
    for (text, kind) in [
        ("cliproxy", ProviderKind::Cliproxy),
        ("openai", ProviderKind::Openai),
        ("gemini", ProviderKind::Gemini),
        ("xai", ProviderKind::Xai),
        ("openai_compatible", ProviderKind::OpenaiCompatible),
    ] {
        assert_eq!(ProviderKind::parse(text), Some(kind));
        assert_eq!(kind.as_str(), text);
        assert!(!kind.label().is_empty());
    }
    assert_eq!(ProviderKind::parse("OpenAI"), None);
    assert!(ProviderKind::Openai.key_required() && ProviderKind::Gemini.key_required() && ProviderKind::Xai.key_required());
    assert!(!ProviderKind::Cliproxy.key_required() && !ProviderKind::OpenaiCompatible.key_required());
    assert_eq!(ProviderKind::Cliproxy.default_concurrency(), 30);
    assert_eq!(ProviderKind::OpenaiCompatible.default_concurrency(), 30);
    assert_eq!(ProviderKind::Gemini.default_concurrency(), 4);
    assert_eq!(ProviderKind::Cliproxy.default_base_url(), None);
}

#[test]
fn transport_warning_rules() {
    for ok in [
        "https://api.openai.com/v1",
        "http://127.0.0.1:8317/v1",
        "http://localhost:1/v1",
        "http://10.0.0.5/v1",
        "http://172.20.1.1/v1",
        "http://192.168.0.2/v1",
        "http://100.98.54.122:8317/v1",
        "http://desk.tail1234.ts.net:8317/v1",
        "http://[::1]:1/v1",
        // MagicDNS short names and other single-label LAN hosts.
        "http://desktop-sb818kq:8317/v1",
        "http://DESKTOP-SB818KQ:8317/v1",
        "http://nas/v1",
        // Tailscale IPv6 (fd7a:115c:a1e0::/48).
        "http://[fd7a:115c:a1e0::1]:8317/v1",
        "http://[fd7a:115c:a1e0:ab12:4843:cd96:6258:b240]:8317/v1",
    ] {
        assert!(transport_warning(ok).is_none(), "{ok}");
    }
    for warn in [
        "http://example.com/v1",
        "http://8.8.8.8/v1",
        "http://100.200.0.1/v1",
        "http://172.32.0.1/v1",
        "http://ts.net.evil.com/v1",
        "http://[fd7a:115c:a1e1::1]/v1",
        "http://[2001:db8::1]/v1",
        "http://desktop.example/v1",
    ] {
        assert!(transport_warning(warn).is_some(), "{warn}");
    }
}

#[test]
fn generation_status_is_recorded_per_profile() {
    let (dir, store) = temp_store();
    assert!(store.generation_status().unwrap().is_empty());
    let failed = GenerationStatus {
        ok: false,
        at: 1.5,
        model: "grok-imagine-image".into(),
        category: Some(Category::Quota),
        provider_code: Some("spend".into()),
    };
    store.record_generation("p-00000001", &failed).unwrap();
    let ok = GenerationStatus { ok: true, at: 2.0, model: "gpt-image-2".into(), category: None, provider_code: None };
    store.record_generation("p-00000002", &ok).unwrap();
    let all = store.generation_status().unwrap();
    assert_eq!(all["p-00000001"].category, Some(Category::Quota));
    assert!(all["p-00000002"].ok);
    store.record_generation("p-00000001", &ok).unwrap();
    assert!(store.generation_status().unwrap()["p-00000001"].ok);
    // status.json is a cache: an unreadable one is replaced, never blocks recording.
    std::fs::write(dir.path().join("status.json"), "garbage").unwrap();
    store.record_generation("p-00000003", &ok).unwrap();
    assert_eq!(store.generation_status().unwrap().len(), 1);
}

#[test]
fn removing_a_profile_drops_its_status() {
    let (_dir, store) = temp_store();
    let p = store.add("a", ProviderKind::Openai, None, None).unwrap();
    let ok = GenerationStatus { ok: true, at: 2.0, model: "gpt-image-2".into(), category: None, provider_code: None };
    store.record_generation(&p.id, &ok).unwrap();
    store.remove("a").unwrap();
    assert!(store.generation_status().unwrap().is_empty());
}

#[test]
fn default_dir_prefers_image_studio_home() {
    let env = MapEnv::new(&[("IMAGE_STUDIO_HOME", "X:\\studio"), ("APPDATA", "Y:\\roaming")], &[]);
    assert_eq!(ProfileStore::default_dir(&env), Some(PathBuf::from("X:\\studio")));
    let env = MapEnv::new(&[("APPDATA", "Y:\\roaming")], &[]);
    assert_eq!(ProfileStore::default_dir(&env), Some(PathBuf::from("Y:\\roaming").join("ImageStudio")));
    assert_eq!(ProfileStore::default_dir(&MapEnv::new(&[], &[])), None);
}
