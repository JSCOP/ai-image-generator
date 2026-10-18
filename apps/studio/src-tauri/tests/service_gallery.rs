//! Gallery over registered folders: records in `ImageGallery/metadata/<topic>/`, temp folders only.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use image_core::error::Category;
use serde_json::{json, Value};
use studio_lib::service::gallery::{add_folder, gallery, normalize_folders, remove_folder, GalleryItem};
use studio_lib::service::settings::{load_settings, save_settings, Settings};

fn settings(work_dir: &Path, extra: &[&Path]) -> Settings {
    Settings {
        work_dir: work_dir.to_path_buf(),
        gallery_folders: extra.iter().map(|p| p.to_path_buf()).collect(),
        last_profile: None,
        last_model: None,
    }
}

fn record(prompt: &str, status: Option<&str>) -> Value {
    let mut value = json!({
        "prompt": prompt, "image_model": "gpt-image-2", "size": "1024x1024", "quality": "low",
        "action": "generate", "reference_images": [], "output": "C:\\moved\\elsewhere.png",
    });
    if let Some(status) = status {
        value["status"] = json!(status);
    }
    value
}

/// Writes `metadata/<topic>/<name>.json` (and the image when asked) with the record's modified time `age` seconds ago.
fn put(workspace: &Path, topic: &str, name: &str, rec: &Value, image: bool, age: u64) -> PathBuf {
    let root = workspace.join("ImageGallery");
    let image_path = root.join("output").join(topic).join(name);
    let record_path = root.join("metadata").join(topic).join(format!("{name}.json"));
    std::fs::create_dir_all(record_path.parent().unwrap()).unwrap();
    std::fs::write(&record_path, serde_json::to_vec_pretty(rec).unwrap()).unwrap();
    let file = std::fs::File::options().write(true).open(&record_path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(age)).unwrap();
    if image {
        std::fs::create_dir_all(image_path.parent().unwrap()).unwrap();
        std::fs::write(&image_path, b"not decoded by the gallery").unwrap();
    }
    image_path
}

fn find<'a>(items: &'a [GalleryItem], prompt: &str) -> &'a GalleryItem {
    items.iter().find(|i| i.prompt == prompt).unwrap_or_else(|| panic!("no item {prompt}"))
}

#[test]
fn lists_complete_failed_pending_legacy_and_missing() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let mut complete = record("완료 그림", Some("complete"));
    complete["response_model"] = json!("gpt-image-2-2026");
    complete["background"] = json!("transparent");
    complete["alpha"] = json!({"has_alpha_channel": true, "transparent_ratio": 0.448, "partial_ratio": 0.01, "opaque_ratio": 0.542, "corner_alpha": [0, 0, 0, 0]});
    let complete_image = put(&work, "t", "a.png", &complete, true, 50);
    let mut failed = record("실패 그림", Some("failed"));
    failed["error"] = json!({"category": "quota", "provider_code": "insufficient_quota", "http_status": 429, "retryable": false, "message": "한도를 넘었습니다"});
    put(&work, "t", "b.png", &failed, false, 40);
    put(&work, "t", "c.png", &record("대기 그림", Some("pending")), false, 30);
    put(&work, "old", "d.png", &record("파이썬 그림", None), true, 20);
    put(&work, "t", "e.png", &record("사라진 그림", Some("complete")), false, 10);

    let view = gallery(&settings(&work, &[]), 2000);
    assert_eq!(view.items.len(), 5, "{:?}", view.items);
    assert!(!view.truncated);

    let item = find(&view.items, "완료 그림");
    assert_eq!(item.status, "complete");
    assert_eq!(item.image_path, complete_image.display().to_string(), "image found next to the record, not at the stale output field");
    assert!(item.record_path.ends_with("a.png.json"));
    assert_eq!(item.topic, "t");
    assert_eq!(item.image_model, "gpt-image-2");
    assert_eq!(item.response_model.as_deref(), Some("gpt-image-2-2026"));
    assert_eq!(item.size, "1024x1024");
    assert_eq!(item.background.as_deref(), Some("transparent"));
    assert_eq!(item.transparent_ratio, Some(0.448));
    assert_eq!(item.error, None);
    assert_eq!(item.folder, view.folders[0].gallery_root);

    let item = find(&view.items, "실패 그림");
    assert_eq!(item.status, "failed");
    assert_eq!(item.error.as_deref(), Some("한도를 넘었습니다"));
    assert_eq!(find(&view.items, "대기 그림").status, "pending");
    assert_eq!(find(&view.items, "파이썬 그림").status, "legacy");
    assert_eq!(find(&view.items, "파이썬 그림").topic, "old");
    assert_eq!(find(&view.items, "사라진 그림").status, "missing");

    assert_eq!(view.folders.len(), 1);
    assert!(view.folders[0].available && view.folders[0].is_work_dir);
}

#[test]
fn newest_first_and_limit_sets_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    for (i, age) in [(1, 300), (2, 100), (3, 200), (4, 50)] {
        put(&work, "t", &format!("{i}.png"), &record(&format!("p{i}"), Some("complete")), true, age);
    }
    let view = gallery(&settings(&work, &[]), 10);
    let order: Vec<&str> = view.items.iter().map(|i| i.prompt.as_str()).collect();
    assert_eq!(order, ["p4", "p2", "p3", "p1"]);
    assert!(view.items.windows(2).all(|w| w[0].modified >= w[1].modified));
    assert!(!view.truncated);

    let view = gallery(&settings(&work, &[]), 2);
    let order: Vec<&str> = view.items.iter().map(|i| i.prompt.as_str()).collect();
    assert_eq!(order, ["p4", "p2"]);
    assert!(view.truncated);
}

#[test]
fn skips_lock_tmp_and_non_json() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    put(&work, "t", "a.png", &record("진짜", Some("complete")), true, 10);
    let meta = work.join("ImageGallery").join("metadata").join("t");
    std::fs::write(meta.join("a.png.lock"), br#"{"pid": 1}"#).unwrap();
    std::fs::write(meta.join("b.png.json.0123abcd.tmp"), serde_json::to_vec(&record("임시", None)).unwrap()).unwrap();
    std::fs::write(meta.join("notes.txt"), b"hello").unwrap();
    std::fs::write(meta.join("c.png.0123.stale"), b"{}").unwrap();
    // Not an image record (no output): e.g. another tool's job.json.
    std::fs::write(meta.join("job.json"), br#"{"id": "x", "status": "succeeded", "prompt": "job"}"#).unwrap();
    // A record directly in metadata/ (not inside a topic) is not part of the layout.
    std::fs::write(work.join("ImageGallery").join("metadata").join("loose.png.json"), serde_json::to_vec(&record("밖", None)).unwrap())
        .unwrap();

    let view = gallery(&settings(&work, &[]), 2000);
    let prompts: Vec<&str> = view.items.iter().map(|i| i.prompt.as_str()).collect();
    assert_eq!(prompts, ["진짜"]);
}

#[test]
fn records_one_folder_below_the_topic_are_listed_deeper_ones_not() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let sub = Path::new("sub").join("a.png");
    let image = put(&work, "t", &sub.to_string_lossy(), &record("한 단계 아래", Some("complete")), true, 10);
    let deeper = Path::new("sub").join("deeper").join("b.png");
    put(&work, "t", &deeper.to_string_lossy(), &record("두 단계 아래", Some("complete")), true, 10);
    let view = gallery(&settings(&work, &[]), 2000);
    let prompts: Vec<&str> = view.items.iter().map(|i| i.prompt.as_str()).collect();
    assert_eq!(prompts, ["한 단계 아래"]);
    assert_eq!(view.items[0].image_path, image.display().to_string());
    assert_eq!(view.items[0].topic, "t");
}

#[test]
fn unavailable_folder_is_reported_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let project = dir.path().join("project");
    let gone = dir.path().join("unplugged-drive").join("project");
    put(&work, "t", "a.png", &record("작업 폴더", Some("complete")), true, 10);
    put(&project, "p", "b.png", &record("프로젝트", Some("complete")), true, 20);

    let view = gallery(&settings(&work, &[&gone, &project]), 2000);
    let prompts: Vec<&str> = view.items.iter().map(|i| i.prompt.as_str()).collect();
    assert_eq!(prompts, ["작업 폴더", "프로젝트"]);
    assert_eq!(view.folders.len(), 3);
    assert!(view.folders[0].is_work_dir && view.folders[0].available);
    let missing = view.folders.iter().find(|f| f.path == gone.display().to_string()).unwrap();
    assert!(!missing.available && !missing.is_work_dir);
    assert!(missing.gallery_root.ends_with("ImageGallery"));
    let ok = view.folders.iter().find(|f| f.path == project.display().to_string()).unwrap();
    assert!(ok.available);
    assert_eq!(find(&view.items, "프로젝트").folder, ok.gallery_root);
}

#[test]
fn work_dir_not_created_yet_is_available_and_empty() {
    let dir = tempfile::tempdir().unwrap();
    let view = gallery(&settings(&dir.path().join("Image Studio"), &[]), 2000);
    assert!(view.items.is_empty());
    assert!(view.folders[0].available, "first run: the work folder is made by the first generation");
    // Its parent is gone too (e.g. an unplugged drive): not available.
    let view = gallery(&settings(&dir.path().join("gone").join("Image Studio"), &[]), 2000);
    assert!(!view.folders[0].available);
}

#[test]
fn normalize_folders_dedupes_case_insensitively_and_maps_to_gallery_root() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("X").join("ImageGallery").join("output");
    let lower = dir.path().join("x");
    let other = dir.path().join("other");
    let roots = normalize_folders(&[output.clone(), lower, other.clone()]);
    assert_eq!(roots.len(), 2, "{roots:?}");
    assert!(roots[0].to_string_lossy().to_lowercase().ends_with(&format!("{}imagegallery", std::path::MAIN_SEPARATOR).to_lowercase()));
    assert!(roots[0].to_string_lossy().to_lowercase().contains(&format!("{}x{}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR)));
    assert!(roots[1].ends_with(Path::new("other").join("ImageGallery")));

    // The work folder and a registered copy of it show once, as the work folder.
    let view = gallery(&settings(&other, &[&other.join("ImageGallery"), &output]), 2000);
    assert_eq!(view.folders.len(), 2);
    assert!(view.folders[0].is_work_dir);
    assert!(!view.folders[1].is_work_dir);
}

#[test]
fn add_and_remove_folder_update_settings() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    save_settings(home.path(), &settings(&work, &[])).unwrap();
    let project = dir.path().join("Project");
    std::fs::create_dir_all(project.join("ImageGallery").join("output")).unwrap();

    let saved = add_folder(home.path(), &project).unwrap();
    assert_eq!(saved.gallery_folders, std::slice::from_ref(&project));
    assert_eq!(load_settings(home.path()), saved);
    // The same gallery again (other spelling, inside the gallery) changes nothing; nor does the work folder.
    let again = add_folder(home.path(), &dir.path().join("project").join("ImageGallery").join("output")).unwrap();
    assert_eq!(again.gallery_folders, std::slice::from_ref(&project));
    assert_eq!(add_folder(home.path(), &work).unwrap().gallery_folders, std::slice::from_ref(&project));

    let err = add_folder(home.path(), Path::new("relative")).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    let err = add_folder(home.path(), &dir.path().join("nowhere")).unwrap_err();
    assert_eq!(err.category, Category::NotFound);
    assert!(err.message.contains("nowhere"), "{}", err.message);

    let err = remove_folder(home.path(), &work).unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    // Removing works by gallery, whatever spelling was registered; a second removal is a no-op.
    let removed = remove_folder(home.path(), &dir.path().join("PROJECT").join("ImageGallery")).unwrap();
    assert!(removed.gallery_folders.is_empty());
    assert!(remove_folder(home.path(), &project).unwrap().gallery_folders.is_empty());
    assert_eq!(load_settings(home.path()).gallery_folders, Vec::<PathBuf>::new());
}

#[test]
fn corrupt_record_is_skipped_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    put(&work, "t", "good.png", &record("멀쩡함", Some("complete")), true, 10);
    let meta = work.join("ImageGallery").join("metadata").join("t");
    std::fs::write(meta.join("broken.png.json"), b"{ not json").unwrap();
    std::fs::write(meta.join("array.png.json"), b"[1, 2]").unwrap();
    std::fs::write(meta.join("status.png.json"), serde_json::to_vec(&json!({"prompt": "x", "output": "y", "status": 3})).unwrap()).unwrap();
    std::fs::write(meta.join("empty.png.json"), b"").unwrap();

    let view = gallery(&settings(&work, &[]), 2000);
    let prompts: Vec<&str> = view.items.iter().map(|i| i.prompt.as_str()).collect();
    assert_eq!(prompts, ["멀쩡함"]);
}
