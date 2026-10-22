//! Galleries found on this PC: `<home>/gallery-index.json`, written by the Python CLI on every run (its `topic_root`)
//! and by "이 PC에서 찾기" (a bounded walk of the fixed drives). The gallery shows them next to the registered folders.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image_core::paths::gallery_root;
use image_core::store::write_json_atomic;
use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "gallery-index.json";

/// How deep the walk goes below a drive or home folder (`C:\Users\me\workspaces\org\project\ImageGallery` is 6).
pub const MAX_DEPTH: usize = 8;
/// The walk stops after this many folders or this long, whichever comes first.
pub const MAX_DIRS: usize = 300_000;
pub const MAX_TIME: Duration = Duration::from_secs(90);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GalleryIndex {
    pub roots: Vec<IndexedRoot>,
    /// Seconds since 1970 of the last "이 PC에서 찾기"; None = never (the gallery screen then runs it once).
    pub scanned_at: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndexedRoot {
    pub path: PathBuf,
    /// Seconds since 1970 when a run wrote there or the walk found it.
    #[serde(default)]
    pub last_used: f64,
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

fn key(path: &Path) -> String {
    path.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase()
}

/// An unreadable or missing file is an empty index.
pub fn load_index(home: &Path) -> GalleryIndex {
    std::fs::read(home.join(FILE_NAME)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// Adds gallery roots (any folder inside a gallery maps to its root); known ones only get a newer `last_used`.
pub fn record_roots(home: &Path, found: &[PathBuf], scanned: bool) -> std::io::Result<GalleryIndex> {
    let mut index = load_index(home);
    let stamp = now();
    for folder in found {
        let root = gallery_root(folder);
        match index.roots.iter_mut().find(|known| key(&known.path) == key(&root)) {
            Some(known) => known.last_used = known.last_used.max(stamp),
            None => index.roots.push(IndexedRoot { path: root, last_used: stamp }),
        }
    }
    if scanned {
        index.scanned_at = Some(stamp);
    }
    write_json_atomic(&home.join(FILE_NAME), &index)?;
    Ok(index)
}

/// Folder names the walk never enters: system, package, build and cache folders.
fn skipped(name: &str) -> bool {
    const SKIP: &[&str] = &[
        "windows", "program files", "program files (x86)", "programdata", "appdata", "recovery", "perflogs",
        "system volume information", "node_modules", "target", "venv", "__pycache__", "site-packages", "dist",
        "build", "msys64", "cygwin64", "intel", "amd", "nvidia",
    ];
    name.starts_with('.') || name.starts_with('$') || SKIP.contains(&name.to_lowercase().as_str())
}

/// Every `ImageGallery` folder with a `metadata` folder below `starts` (not inside one, no links followed).
pub fn find_galleries(starts: &[PathBuf], max_depth: usize, max_dirs: usize, max_time: Duration) -> Vec<PathBuf> {
    let started = Instant::now();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = starts.iter().map(|s| (s.clone(), 0)).collect();
    let mut visited = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        visited += 1;
        if visited > max_dirs || started.elapsed() > max_time {
            break;
        }
        if !seen.insert(key(&dir)) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            // Junctions and symlinks report as symlinks: never followed, so no loops and no double counting.
            let Ok(kind) = entry.file_type() else { continue };
            if !kind.is_dir() || kind.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if name.eq_ignore_ascii_case("imagegallery") {
                if path.join("metadata").is_dir() {
                    found.push(path);
                }
                continue;
            }
            if depth + 1 < max_depth && !skipped(&name) {
                queue.push_back((path, depth + 1));
            }
        }
    }
    found
}

/// Where "이 PC에서 찾기" starts: every fixed (local, non-removable, non-network) drive.
pub fn search_starts() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        const DRIVE_FIXED: u32 = 3;
        (b'A'..=b'Z')
            .map(|letter| format!("{}:\\", letter as char))
            .filter(|root| {
                let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
                // SAFETY: a NUL-terminated UTF-16 string that lives for the call.
                unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
            })
            .map(PathBuf::from)
            .collect()
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir().into_iter().collect()
    }
}
