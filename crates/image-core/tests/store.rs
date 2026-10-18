use image_core::error::{Category, EngineError};
use image_core::normalize::Fit;
use image_core::provider::{Action, Background, ImageRequest};
use image_core::size::Size;
use image_core::store::{inspect_existing, request_hash, reserve, sha256_hex, take_over_stale_lock, write_json_atomic, Existing, Record};
use std::path::{Path, PathBuf};
use test_fixture::png_bytes;

struct Dirs {
    _tmp: tempfile::TempDir,
    out: PathBuf,
    rec: PathBuf,
    lock_dir: PathBuf,
}

fn dirs() -> Dirs {
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("ImageGallery/output/t");
    let lock_dir = tmp.path().join("ImageGallery/metadata/t");
    std::fs::create_dir_all(&out_dir).unwrap();
    Dirs { out: out_dir.join("a.png"), rec: lock_dir.join("a.png.json"), lock_dir, _tmp: tmp }
}

fn request(prompt: &str) -> ImageRequest {
    ImageRequest {
        prompt: prompt.into(),
        image_model: "gpt-image-2.5".into(),
        size: Size { width: 8, height: 8 },
        quality: "high".into(),
        action: Action::Generate,
        references: vec![],
        background: Background::Auto,
    }
}

fn record(out: &Path, hash: &str) -> Record {
    Record {
        prompt: "p".into(),
        image_model: "gpt-image-2.5".into(),
        size: "8x8".into(),
        quality: "high".into(),
        action: "generate".into(),
        output: out.display().to_string(),
        request_hash: Some(hash.into()),
        ..Record::default()
    }
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn second_reservation_of_same_output_conflicts() {
    let d = dirs();
    let r1 = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    assert_eq!(reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap_err().category, Category::Conflict);
    drop(r1);
    assert!(reserve(&d.out, Some(&d.rec), &d.lock_dir).is_ok());
}

#[test]
fn stale_lock_from_dead_process_is_reclaimed() {
    let d = dirs();
    std::fs::create_dir_all(&d.lock_dir).unwrap();
    std::fs::write(d.lock_dir.join("a.png.lock"), r#"{"pid": 4294967000, "request_id": "x", "created_at": 1}"#).unwrap();
    assert!(reserve(&d.out, Some(&d.rec), &d.lock_dir).is_ok());
}

#[test]
fn publish_never_overwrites_existing_file() {
    let d = dirs();
    std::fs::write(&d.out, b"keep me").unwrap();
    let r = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    let err = r.publish(&png_bytes(8, 8, [1, 1, 1, 255]), record(&d.out, "h")).unwrap_err();
    assert_eq!(err.category, Category::Conflict);
    assert_eq!(std::fs::read(&d.out).unwrap(), b"keep me");
}

#[test]
fn publish_writes_complete_record_with_hash() {
    let d = dirs();
    let png = png_bytes(8, 8, [1, 1, 1, 255]);
    let r = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    r.write_pending(&record(&d.out, "h")).unwrap();
    assert_eq!(read_json(&d.rec)["status"], "pending");
    let done = r.publish(&png, record(&d.out, "h")).unwrap();
    assert_eq!(std::fs::read(&d.out).unwrap(), png);
    let saved = read_json(&d.rec);
    assert_eq!(saved["status"], "complete");
    assert_eq!(saved["sha256"], sha256_hex(&png));
    assert_eq!(done.sha256.as_deref(), Some(sha256_hex(&png).as_str()));
    assert!(saved["request_id"].is_string());
    assert!(!d.lock_dir.join("a.png.lock").exists());
    let leftovers: Vec<_> = std::fs::read_dir(d.out.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers.len(), 1, "only the final image remains: {leftovers:?}");
}

#[test]
fn failed_generation_records_error_and_no_image() {
    let d = dirs();
    let r = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    r.write_pending(&record(&d.out, "h")).unwrap();
    r.fail(record(&d.out, "h"), &EngineError::new(Category::Quota, "HTTP 403"));
    let saved = read_json(&d.rec);
    assert_eq!(saved["status"], "failed");
    assert_eq!(saved["error"]["category"], "quota");
    assert!(!d.out.exists());
}

#[test]
fn interrupted_write_leaves_no_final_file() {
    let d = dirs();
    let r = reserve(&d.out, None, &d.lock_dir).unwrap();
    drop(r);
    assert!(!d.out.exists());
    assert!(!d.lock_dir.join("a.png.lock").exists());
    assert_eq!(std::fs::read_dir(d.out.parent().unwrap()).unwrap().count(), 0);
    assert!(!d.rec.exists(), "record_metadata=false writes no record");
}

#[test]
fn inspect_rules() {
    let d = dirs();
    let png = png_bytes(8, 8, [1, 1, 1, 255]);
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Absent));
    std::fs::write(&d.out, &png).unwrap();
    // No record or a Python record without status: reusable when the image decodes.
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Reusable));
    write_json_atomic(&d.rec, &serde_json::json!({"prompt": "p", "output": "x"})).unwrap();
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Reusable));
    let mut complete = record(&d.out, "h");
    complete.status = Some("complete".into());
    complete.sha256 = Some(sha256_hex(&png));
    write_json_atomic(&d.rec, &complete).unwrap();
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Reusable));
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "other"), Existing::Mismatch(_)));
    let mut pending = record(&d.out, "h");
    pending.status = Some("pending".into());
    write_json_atomic(&d.rec, &pending).unwrap();
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::NeedsFinalize));
    write_json_atomic(&d.rec, &complete).unwrap();
    std::fs::write(&d.out, b"tampered").unwrap();
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Corrupt(_)));
    std::fs::remove_file(&d.rec).unwrap();
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Corrupt(_)));
}

#[test]
fn request_hash_changes_with_prompt_model_size_refs_background_fit() {
    let base = request_hash(&request("p"), Fit::Cover).unwrap();
    assert_eq!(base, request_hash(&request("p"), Fit::Cover).unwrap());
    assert_ne!(base, request_hash(&request("q"), Fit::Cover).unwrap());
    assert_ne!(base, request_hash(&request("p"), Fit::Contain).unwrap());
    let mut r = request("p");
    r.image_model = "gemini-3.1-flash-image".into();
    assert_ne!(base, request_hash(&r, Fit::Cover).unwrap());
    let mut r = request("p");
    r.size = Size { width: 9, height: 8 };
    assert_ne!(base, request_hash(&r, Fit::Cover).unwrap());
    let mut r = request("p");
    r.background = Background::Transparent;
    assert_ne!(base, request_hash(&r, Fit::Cover).unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let reference = tmp.path().join("r.png");
    std::fs::write(&reference, b"one").unwrap();
    let mut r = request("p");
    r.references = vec![reference.clone()];
    let with_ref = request_hash(&r, Fit::Cover).unwrap();
    assert_ne!(base, with_ref);
    std::fs::write(&reference, b"two").unwrap();
    assert_ne!(with_ref, request_hash(&r, Fit::Cover).unwrap());
}

fn lock_path(d: &Dirs) -> PathBuf {
    d.lock_dir.join("a.png.lock")
}

#[test]
fn takeover_does_not_steal_a_lock_that_changed_owner() {
    let d = dirs();
    std::fs::create_dir_all(&d.lock_dir).unwrap();
    let dead = br#"{"pid": 4294967000, "request_id": "dead", "created_at": 1}"#;
    let live = format!(r#"{{"pid": {}, "request_id": "live", "created_at": 2}}"#, std::process::id());
    std::fs::write(lock_path(&d), &live).unwrap();
    assert!(!take_over_stale_lock(&lock_path(&d), dead), "content changed since judged stale");
    assert_eq!(std::fs::read_to_string(lock_path(&d)).unwrap(), live);
    std::fs::write(lock_path(&d), dead).unwrap();
    assert!(take_over_stale_lock(&lock_path(&d), dead));
    assert!(!lock_path(&d).exists());
}

#[test]
fn release_leaves_a_lock_owned_by_another_run() {
    let d = dirs();
    let r = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    std::fs::write(lock_path(&d), r#"{"pid": 1, "request_id": "someone-else", "created_at": 3}"#).unwrap();
    drop(r);
    assert!(lock_path(&d).exists());
}

#[test]
fn conflicting_publish_keeps_the_other_runs_complete_record() {
    let d = dirs();
    let r = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    let png = png_bytes(8, 8, [1, 1, 1, 255]);
    std::fs::write(&d.out, &png).unwrap();
    let mut other = record(&d.out, "h");
    other.status = Some("complete".into());
    other.sha256 = Some(sha256_hex(&png));
    write_json_atomic(&d.rec, &other).unwrap();
    let err = r.publish(&png_bytes(8, 8, [2, 2, 2, 255]), record(&d.out, "h")).unwrap_err();
    r.fail(record(&d.out, "h"), &err);
    assert_eq!(read_json(&d.rec)["status"], "complete");
    assert_eq!(std::fs::read(&d.out).unwrap(), png);
}

#[test]
fn pending_or_failed_record_with_matching_image_needs_finalize() {
    let d = dirs();
    let png = png_bytes(8, 8, [1, 1, 1, 255]);
    std::fs::write(&d.out, &png).unwrap();
    for status in ["pending", "failed"] {
        let mut rec = record(&d.out, "h");
        rec.status = Some(status.into());
        write_json_atomic(&d.rec, &rec).unwrap();
        assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::NeedsFinalize), "{status}");
    }
    let r = reserve(&d.out, Some(&d.rec), &d.lock_dir).unwrap();
    let done = r.finalize_existing().unwrap();
    let saved = read_json(&d.rec);
    assert_eq!(saved["status"], "complete");
    assert_eq!(saved["sha256"], sha256_hex(&png));
    assert_eq!(saved["saved_size"], "8x8");
    assert!(saved.get("error").is_none());
    assert_eq!(done.status.as_deref(), Some("complete"));
    assert!(!lock_path(&d).exists());
    assert!(matches!(inspect_existing(&d.out, Some(&d.rec), "h"), Existing::Reusable));
}
