//! Image Studio desktop app: a thin Tauri shell over `image-core`.

pub mod commands;
pub mod service;
pub mod state;

/// Every command the webview may call (names match `src/lib/api.ts`).
macro_rules! handlers {
    () => {
        tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::connections,
            commands::save_profile,
            commands::remove_profile,
            commands::set_default_profile,
            commands::check_profile,
            commands::list_models,
        ]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::system())
        .invoke_handler(handlers!())
        .run(tauri::generate_context!())
        .expect("error while running Image Studio");
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use image_core::secrets::{MemoryStore, SecretStore};
    use serde_json::{json, Value};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::WebviewWindow;
    use test_fixture::MapEnv;

    use super::*;
    use crate::service::Ctx;
    use crate::state::{app_home, AppState};

    fn app(home: &Path, secrets: Arc<MemoryStore>) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
        let ctx = Ctx { home: home.to_path_buf(), env: Arc::new(MapEnv::new(&[], &[])), secrets };
        let app = mock_builder().manage(AppState::new(ctx)).invoke_handler(handlers!()).build(mock_context(noop_assets())).unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        (app, webview)
    }

    fn invoke(webview: &WebviewWindow<MockRuntime>, cmd: &str, body: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(webview, request).map(|r| r.deserialize::<Value>().unwrap())
    }

    #[test]
    fn app_info_command_returns_version_home_and_work_dir() {
        let home = tempfile::tempdir().unwrap();
        let (_app, webview) = app(home.path(), Arc::new(MemoryStore::new()));
        let body = invoke(&webview, "app_info", json!({})).unwrap();
        let object = body.as_object().unwrap();
        assert_eq!(object.keys().collect::<Vec<_>>(), ["version", "home", "workDir"]);
        assert_eq!(body["version"], "0.1.0");
        assert_eq!(body["home"], home.path().to_string_lossy().as_ref());
        assert!(body["workDir"].as_str().unwrap().ends_with("Image Studio"));
    }

    #[test]
    fn app_home_follows_image_studio_home() {
        let appdata = ("APPDATA", "C:\\Users\\x\\AppData\\Roaming");
        let env = MapEnv::new(&[("IMAGE_STUDIO_HOME", "D:\\tmp\\studio-home"), appdata], &[]);
        assert_eq!(app_home(&env), PathBuf::from("D:\\tmp\\studio-home"));
        let env = MapEnv::new(&[appdata], &[]);
        assert_eq!(app_home(&env), PathBuf::from("C:\\Users\\x\\AppData\\Roaming").join("ImageStudio"));
        // Neither set: the OS config folder, never an empty path.
        assert!(app_home(&MapEnv::new(&[], &[])).ends_with("ImageStudio"));
    }

    #[test]
    fn profile_and_settings_commands_round_trip_without_the_key() {
        let home = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemoryStore::new());
        let (_app, webview) = app(home.path(), secrets.clone());
        let key = "sk-ipc-secret-987654321";
        let input = json!({"name": "local", "kind": "cliproxy", "baseUrl": "http://127.0.0.1:8317", "key": key, "makeDefault": true});
        let saved = invoke(&webview, "save_profile", json!({ "input": input })).unwrap();
        assert!(!saved.to_string().contains(key), "{saved}");
        assert_eq!(saved["keyConfigured"], true);
        assert_eq!(saved["isDefault"], true);
        let id = saved["id"].as_str().unwrap().to_string();
        assert_eq!(secrets.get(&id).unwrap().unwrap().expose(), key);

        let view = invoke(&webview, "connections", json!({})).unwrap();
        assert_eq!(view["active"], "local");
        assert!(!view.to_string().contains(key));

        let err = invoke(&webview, "remove_profile", json!({"id": "p-00000000"})).unwrap_err();
        assert_eq!(err["category"], "not_found");
        assert!(err["message"].as_str().unwrap().contains("p-00000000"));
        invoke(&webview, "set_default_profile", json!({"id": null})).unwrap();
        assert_eq!(invoke(&webview, "connections", json!({})).unwrap()["active"], Value::Null);
        invoke(&webview, "remove_profile", json!({ "id": id })).unwrap();
        assert!(secrets.get(&id).unwrap().is_none());

        let mut settings = invoke(&webview, "get_settings", json!({})).unwrap();
        assert!(settings["workDir"].as_str().unwrap().ends_with("Image Studio"));
        settings["lastModel"] = json!("gpt-image-2");
        invoke(&webview, "save_settings", json!({ "settings": settings })).unwrap();
        assert_eq!(invoke(&webview, "get_settings", json!({})).unwrap()["lastModel"], "gpt-image-2");
        let err = invoke(&webview, "save_settings", json!({"settings": {"workDir": "relative"}})).unwrap_err();
        assert_eq!(err["category"], "invalid_request");
    }
}
