//! App settings in `<home>/settings.json` (work folder, extra gallery folders, last choices).

use std::path::{Path, PathBuf};

use image_core::error::Category;
use image_core::store::write_json_atomic;
use serde::{Deserialize, Serialize};

use super::ServiceError;

const FILE_NAME: &str = "settings.json";

/// Missing fields take their defaults; the work folder is always shown in the gallery, `gallery_folders` are extra ones.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub work_dir: PathBuf,
    pub gallery_folders: Vec<PathBuf>,
    pub last_profile: Option<String>,
    pub last_model: Option<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { work_dir: default_work_dir(), gallery_folders: Vec::new(), last_profile: None, last_model: None }
    }
}

/// `%USERPROFILE%\Pictures\Image Studio` (falls back to `<home>\Pictures`, then the current folder).
pub fn default_work_dir() -> PathBuf {
    let pictures = dirs::picture_dir().or_else(|| dirs::home_dir().map(|home| home.join("Pictures"))).unwrap_or_else(|| PathBuf::from("."));
    pictures.join("Image Studio")
}

/// Defaults when the file is missing or unreadable; an unreadable file is moved to `settings.json.bad-<time>` first.
pub fn load_settings(home: &Path) -> Settings {
    let path = home.join(FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(_) => {
            move_aside(&path);
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(mut settings) => {
            if settings.work_dir.as_os_str().is_empty() {
                settings.work_dir = default_work_dir();
            }
            settings
        }
        Err(_) => {
            move_aside(&path);
            Settings::default()
        }
    }
}

fn move_aside(path: &Path) {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut target = path.with_file_name(format!("{FILE_NAME}.bad-{stamp}"));
    let mut n = 1;
    while target.exists() {
        target = path.with_file_name(format!("{FILE_NAME}.bad-{stamp}-{n}"));
        n += 1;
    }
    // If even the move fails the next save replaces the file; defaults are used either way.
    let _ = std::fs::rename(path, target);
}

pub fn save_settings(home: &Path, settings: &Settings) -> Result<(), ServiceError> {
    if !settings.work_dir.is_absolute() {
        return Err(ServiceError::new(
            Category::InvalidRequest,
            "작업 폴더는 전체 경로(예: C:\\Users\\이름\\Pictures\\Image Studio)여야 합니다.",
        ));
    }
    if let Some(folder) = settings.gallery_folders.iter().find(|f| !f.is_absolute()) {
        return Err(ServiceError::new(Category::InvalidRequest, format!("갤러리 폴더는 전체 경로여야 합니다: {}", folder.display())));
    }
    let path = home.join(FILE_NAME);
    write_json_atomic(&path, settings)
        .map_err(|e| ServiceError::new(Category::Internal, format!("설정 파일을 저장할 수 없습니다: {}: {e}", path.display())))
}
