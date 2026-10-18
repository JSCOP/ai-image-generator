//! `ImageGallery/output|metadata/<topic>` layout (`scripts/gallery.py`).

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const DEFAULT_TOPIC: &str = "image-request";

static NON_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\w\s.-]+").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Filesystem-safe slug that keeps Korean/Unicode words; same rules as `safe_topic_slug`.
pub fn safe_topic_slug(value: &str) -> String {
    let raw = NON_WORD.replace_all(value.trim(), "_");
    let raw = SPACES.replace_all(&raw, "_");
    let raw = raw.trim_matches(|c| matches!(c, '.' | '_' | '-' | ' '));
    if raw.is_empty() {
        return DEFAULT_TOPIC.to_string();
    }
    raw.chars().take(80).collect()
}

/// Like Python `Path.resolve()`: canonical when the path exists, without the `\?\` prefix.
pub fn absolute(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.to_path_buf());
    strip_verbatim(path)
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// Accepts a workspace, an existing gallery, or a folder inside a gallery.
pub fn gallery_root(workspace: &Path) -> PathBuf {
    let root = absolute(workspace);
    for parent in root.ancestors() {
        let is_gallery = parent.file_name().is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("imagegallery"));
        if is_gallery {
            return parent.to_path_buf();
        }
    }
    root.join("ImageGallery")
}

pub fn topic_dirs(workspace: &Path, topic: &str) -> (PathBuf, PathBuf) {
    let root = gallery_root(workspace);
    (root.join("output").join(topic), root.join("metadata").join(topic))
}

/// `metadata/<topic>/<path relative to output/<topic>>.json`.
pub fn metadata_path(output: &Path, workspace: &Path, topic: &str) -> PathBuf {
    let (images, records) = topic_dirs(workspace, topic);
    let output = absolute(output);
    let relative = match output.strip_prefix(&images) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => PathBuf::from(output.file_name().unwrap_or_default()),
    };
    let mut name = relative.into_os_string();
    name.push(".json");
    records.join(name)
}
