//! Image Studio desktop app: a thin Tauri shell over `image-core`.

use std::path::PathBuf;

use image_core::connection::{EnvSource, SystemEnv};
use image_core::profiles::ProfileStore;
use serde::Serialize;

/// What the app shows about itself: version, app home and default work folder.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// `IMAGE_STUDIO_HOME`, else `%APPDATA%\ImageStudio`; `None` when neither is set.
    pub home: Option<PathBuf>,
    pub work_dir: PathBuf,
}

/// `%USERPROFILE%\Pictures\Image Studio` (falls back to `<home>\Pictures`, then the current folder).
pub fn default_work_dir() -> PathBuf {
    let pictures = dirs::picture_dir().or_else(|| dirs::home_dir().map(|home| home.join("Pictures"))).unwrap_or_else(|| PathBuf::from("."));
    pictures.join("Image Studio")
}

pub fn app_info_from(env: &dyn EnvSource) -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION").to_string(), home: ProfileStore::default_dir(env), work_dir: default_work_dir() }
}

#[tauri::command]
fn app_info() -> AppInfo {
    app_info_from(&SystemEnv)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running Image Studio");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use test_fixture::MapEnv;

    use super::*;

    #[test]
    fn app_info_command_returns_version_home_and_work_dir() {
        let app = mock_builder().invoke_handler(tauri::generate_handler![app_info]).build(mock_context(noop_assets())).unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        let request = InvokeRequest {
            cmd: "app_info".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        let body = get_ipc_response(&webview, request).unwrap().deserialize::<serde_json::Value>().unwrap();
        let object = body.as_object().unwrap();
        assert_eq!(object.keys().collect::<Vec<_>>(), ["version", "home", "workDir"]);
        assert_eq!(body["version"], "0.1.0");
        assert!(body["workDir"].as_str().unwrap().ends_with("Image Studio"));
    }

    #[test]
    fn app_info_home_follows_image_studio_home() {
        let appdata = ("APPDATA", "C:\\Users\\x\\AppData\\Roaming");
        let env = MapEnv::new(&[("IMAGE_STUDIO_HOME", "D:\\tmp\\studio-home"), appdata], &[]);
        assert_eq!(app_info_from(&env).home, Some(PathBuf::from("D:\\tmp\\studio-home")));

        let env = MapEnv::new(&[appdata], &[]);
        assert_eq!(app_info_from(&env).home, Some(PathBuf::from("C:\\Users\\x\\AppData\\Roaming").join("ImageStudio")));
        assert_eq!(app_info_from(&MapEnv::new(&[], &[])).home, None);
    }
}
