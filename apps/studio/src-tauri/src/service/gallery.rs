//! Gallery over registered folders: every list is rebuilt from `ImageGallery/metadata/<topic>/*.json` (spec §7.5, no DB).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use image_core::error::Category;
use image_core::paths::gallery_root;
use serde::Serialize;
use serde_json::Value;

use super::settings::{load_settings, save_settings, Settings};
use super::ServiceError;

/// Items the gallery command returns when the screen gives no limit.
pub const DEFAULT_LIMIT: usize = 2000;

/// One record (and its image, when it is there).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub image_path: String,
    pub record_path: String,
    /// `gallery_root` of the folder it came from (same as `FolderView.gallery_root`).
    pub folder: String,
    pub topic: String,
    /// complete | pending | failed | legacy (Python record, no status) | missing (record without its image).
    pub status: String,
    pub prompt: String,
    pub image_model: String,
    pub response_model: Option<String>,
    pub size: String,
    pub background: Option<String>,
    pub transparent_ratio: Option<f64>,
    pub error: Option<String>,
    /// Record file modified time, seconds since 1970.
    pub modified: f64,
    pub quality: Option<String>,
    /// `reference_images` of the record (paths as written then).
    pub references: Vec<String>,
    /// Size the provider sent, before fitting to `saved_size`.
    pub provider_size: Option<String>,
    pub saved_size: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    /// The folder as registered (the work folder for `is_work_dir`).
    pub path: String,
    pub gallery_root: String,
    /// False when the folder is gone (e.g. unplugged drive) or cannot be read.
    pub available: bool,
    pub is_work_dir: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GalleryView {
    pub items: Vec<GalleryItem>,
    pub folders: Vec<FolderView>,
    /// More records exist than `limit`.
    pub truncated: bool,
}

/// Comparison key: Windows paths ignore case and a trailing separator.
fn key(path: &Path) -> String {
    path.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase()
}

/// Gallery roots of `folders` in order, without repeats (case-insensitive); the first spelling wins.
pub fn normalize_folders(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    folders.iter().map(|folder| gallery_root(folder)).filter(|root| seen.insert(key(root))).collect()
}

/// (registered path, gallery root, is work folder): the work folder first, then the extra folders, without repeats.
fn registered(settings: &Settings) -> Vec<(PathBuf, PathBuf, bool)> {
    let mut seen = HashSet::new();
    std::iter::once((&settings.work_dir, true))
        .chain(settings.gallery_folders.iter().map(|folder| (folder, false)))
        .filter_map(|(folder, is_work)| {
            let root = gallery_root(folder);
            seen.insert(key(&root)).then(|| (folder.clone(), root, is_work))
        })
        .collect()
}

/// Gallery roots the screen may show images from (asset protocol scope).
pub fn gallery_roots(settings: &Settings) -> Vec<PathBuf> {
    registered(settings).into_iter().map(|(_, root, _)| root).collect()
}

/// A folder without records yet is available when it exists; the work folder also when only its parent does
/// (the first generation creates it).
fn available(root: &Path, is_work_dir: bool) -> bool {
    match std::fs::read_dir(root.join("metadata")) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let workspace = root.parent();
            root.is_dir()
                || workspace.is_some_and(Path::is_dir)
                || (is_work_dir && workspace.and_then(Path::parent).is_some_and(Path::is_dir))
        }
        Err(_) => false,
    }
}

/// Newest first; folders that are gone or unreadable are reported, never fatal.
pub fn gallery(settings: &Settings, limit: usize) -> GalleryView {
    let mut items = Vec::new();
    let mut folders = Vec::new();
    for (path, root, is_work_dir) in registered(settings) {
        let available = available(&root, is_work_dir);
        if available {
            scan_gallery(&root, &mut items);
        }
        folders.push(FolderView { path: path.display().to_string(), gallery_root: root.display().to_string(), available, is_work_dir });
    }
    items.sort_by(|a, b| b.modified.total_cmp(&a.modified).then_with(|| a.record_path.cmp(&b.record_path)));
    let truncated = items.len() > limit;
    items.truncate(limit);
    GalleryView { items, folders, truncated }
}

fn entries(dir: &Path) -> impl Iterator<Item = (PathBuf, bool)> {
    std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|entry| {
        let is_dir = entry.file_type().ok()?.is_dir();
        Some((entry.path(), is_dir))
    })
}

fn is_record_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".json"))
}

/// `metadata/<topic>/*.json` and `metadata/<topic>/<sub>/*.json`; nothing deeper, nothing else.
fn scan_gallery(root: &Path, items: &mut Vec<GalleryItem>) {
    let metadata = root.join("metadata");
    for (topic_dir, is_dir) in entries(&metadata) {
        if !is_dir {
            continue;
        }
        let topic = topic_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        for (path, is_dir) in entries(&topic_dir) {
            let files: Vec<PathBuf> =
                if is_dir { entries(&path).filter(|(_, is_dir)| !is_dir).map(|(file, _)| file).collect() } else { vec![path] };
            for file in files.into_iter().filter(|file| is_record_file(file)) {
                if let Some(item) = read_item(root, &topic, &topic_dir, &file) {
                    items.push(item);
                }
            }
        }
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// None for anything that is not an image record (unreadable, not an object, non-string status, no `output`).
fn read_item(root: &Path, topic: &str, topic_dir: &Path, file: &Path) -> Option<GalleryItem> {
    let bytes = std::fs::read(file).ok()?;
    let record: Value = serde_json::from_slice(&bytes).ok()?;
    if !record.is_object() {
        return None;
    }
    let status = match record.get("status") {
        None | Some(Value::Null) => None,
        Some(Value::String(status)) => Some(status.clone()),
        Some(_) => return None,
    };
    let output = text(&record, "output").filter(|output| !output.is_empty())?;
    let modified = std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_secs_f64());

    // The image sits beside its record in output/<topic>/; the `output` field is the path at write time (folder may have moved).
    let relative = file.strip_prefix(topic_dir).ok()?.to_string_lossy().to_string();
    let beside = root.join("output").join(topic).join(relative.strip_suffix(".json")?);
    let written = PathBuf::from(&output);
    let image = if !beside.is_file() && written.is_file() { written } else { beside };
    let has_image = image.is_file();

    let status = match status.as_deref() {
        Some("failed") => "failed",
        Some("pending") => "pending",
        Some("complete") if has_image => "complete",
        None if has_image => "legacy",
        // A status this app does not know: judge by the image like a Python record.
        Some(other) if has_image && other != "complete" => "legacy",
        _ => "missing",
    };
    let error = match record.get("error") {
        Some(Value::String(message)) => Some(message.clone()),
        Some(error) => error.get("message").and_then(Value::as_str).map(str::to_string),
        None => None,
    };
    Some(GalleryItem {
        image_path: image.display().to_string(),
        record_path: file.display().to_string(),
        folder: root.display().to_string(),
        topic: topic.to_string(),
        status: status.to_string(),
        prompt: text(&record, "prompt").unwrap_or_default(),
        image_model: text(&record, "image_model").unwrap_or_default(),
        response_model: text(&record, "response_model"),
        size: text(&record, "size").unwrap_or_default(),
        background: text(&record, "background"),
        transparent_ratio: record.get("alpha").and_then(|alpha| alpha.get("transparent_ratio")).and_then(Value::as_f64),
        error,
        modified,
        quality: text(&record, "quality"),
        references: record
            .get("reference_images")
            .and_then(Value::as_array)
            .map(|paths| paths.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        provider_size: text(&record, "provider_size"),
        saved_size: text(&record, "saved_size"),
        request_id: text(&record, "request_id"),
    })
}

/// Registers an existing folder (workspace, gallery or a folder inside one); the same gallery twice changes nothing.
pub fn add_folder(home: &Path, folder: &Path) -> Result<Settings, ServiceError> {
    if !folder.is_absolute() {
        return Err(ServiceError::new(Category::InvalidRequest, format!("갤러리 폴더는 전체 경로여야 합니다: {}", folder.display())));
    }
    if !folder.is_dir() {
        return Err(ServiceError::new(Category::NotFound, format!("폴더를 찾을 수 없습니다: {}", folder.display())));
    }
    let mut settings = load_settings(home);
    let root = key(&gallery_root(folder));
    if gallery_roots(&settings).iter().any(|known| key(known) == root) {
        return Ok(settings);
    }
    settings.gallery_folders.push(folder.to_path_buf());
    save_settings(home, &settings)?;
    Ok(settings)
}

/// Unregisters every entry of that gallery (any spelling); the work folder stays (it changes in settings).
pub fn remove_folder(home: &Path, folder: &Path) -> Result<Settings, ServiceError> {
    let mut settings = load_settings(home);
    let root = key(&gallery_root(folder));
    if key(&gallery_root(&settings.work_dir)) == root {
        return Err(ServiceError::new(
            Category::InvalidRequest,
            "작업 폴더는 갤러리에서 뺄 수 없습니다. 작업 폴더는 설정에서 바꿀 수 있습니다.",
        ));
    }
    let before = settings.gallery_folders.len();
    settings.gallery_folders.retain(|known| key(&gallery_root(known)) != root);
    if settings.gallery_folders.len() != before {
        save_settings(home, &settings)?;
    }
    Ok(settings)
}
