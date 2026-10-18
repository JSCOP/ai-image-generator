//! `ImageGallery/output|metadata/<topic>` layout (`scripts/gallery.py`).

use crate::connection::EnvSource;
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

/// Python `Path(value).expanduser()`: `~` and `~/rest` use the home folder (`USERPROFILE`, else
/// `HOMEDRIVE`+`HOMEPATH` on Windows; `HOME` elsewhere); on Windows `~name/rest` becomes a sibling of the home
/// folder as `ntpath` does. Anything else, or no known home, is returned unchanged.
pub fn expand_user(value: &str, env: &dyn EnvSource) -> String {
    let Some(rest) = value.strip_prefix('~') else { return value.to_string() };
    let is_sep = |c: char| c == '/' || (cfg!(windows) && c == '\\');
    let split = rest.find(is_sep).unwrap_or(rest.len());
    let (user, tail) = rest.split_at(split);
    let home = if cfg!(windows) {
        env.process("USERPROFILE").or_else(|| env.process("HOMEPATH").map(|path| env.process("HOMEDRIVE").unwrap_or_default() + &path))
    } else {
        env.process("HOME")
    };
    let Some(home) = home else { return value.to_string() };
    if user.is_empty() {
        return format!("{home}{tail}");
    }
    if !cfg!(windows) {
        return value.to_string();
    }
    let current = env.process("USERNAME");
    if current.as_deref() == Some(user) {
        return format!("{home}{tail}");
    }
    let home = PathBuf::from(home);
    if current.as_deref() != home.file_name().and_then(|n| n.to_str()) {
        return value.to_string();
    }
    format!("{}{tail}", home.parent().unwrap_or(&home).join(user).display())
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
