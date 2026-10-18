//! Image Studio desktop app: a thin Tauri shell over `image-core`.

pub mod commands;
pub mod service;
pub mod state;

use std::time::Duration;

use tauri::{Manager, WindowEvent};

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
            commands::start_generation,
            commands::cancel_generation,
            commands::inspect_reference,
            commands::gallery,
            commands::add_gallery_folder,
            commands::remove_gallery_folder,
        ]
    };
}

/// How long closing the window waits for cancelled runs to release their files.
const CLOSE_WAIT: Duration = Duration::from_secs(10);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::system())
        .invoke_handler(handlers!())
        .setup(|app| {
            // Images of the registered galleries may show from the start.
            let settings = service::settings::load_settings(&app.state::<state::AppState>().ctx.home);
            commands::allow_galleries(app.handle(), &settings);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window cancels every run and waits (bounded) until they have released their locks.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let runs = window.state::<state::AppState>().runs.clone();
                if runs.cancel_all() > 0 {
                    api.prevent_close();
                    let window = window.clone();
                    tauri::async_runtime::spawn(async move {
                        runs.wait_idle(CLOSE_WAIT).await;
                        let _ = window.destroy();
                    });
                }
            }
        })
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
    use tauri::{Manager, WebviewWindow};
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
    fn generation_commands_validate_and_cancel_by_run_id() {
        let home = tempfile::tempdir().unwrap();
        let (app, webview) = app(home.path(), Arc::new(MemoryStore::new()));
        let input = json!({"prompt": "cat", "imageModel": "gpt-image-2.5", "size": "16x16", "quality": "low", "count": 0});
        let err = invoke(&webview, "start_generation", json!({"input": input, "onEvent": "__CHANNEL__:7"})).unwrap_err();
        assert_eq!(err["category"], "invalid_request");
        assert_eq!(app.state::<AppState>().runs.active(), 0);

        assert_eq!(invoke(&webview, "cancel_generation", json!({"runId": "run-missing"})).unwrap(), false);
        let guard = app.state::<AppState>().runs.register("run-1");
        assert_eq!(invoke(&webview, "cancel_generation", json!({"runId": "run-1"})).unwrap(), true);
        assert!(guard.token().is_cancelled());
    }

    #[test]
    fn gallery_commands_list_register_and_allow_folders_for_the_asset_protocol() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (app, webview) = app(home.path(), Arc::new(MemoryStore::new()));
        let work = dir.path().join("work");
        let records = work.join("ImageGallery").join("metadata").join("t");
        std::fs::create_dir_all(&records).unwrap();
        std::fs::write(records.join("a.png.json"), json!({"prompt": "고양이", "output": "a.png"}).to_string()).unwrap();
        let settings = json!({"workDir": work, "galleryFolders": []});
        invoke(&webview, "save_settings", json!({ "settings": settings })).unwrap();

        let view = invoke(&webview, "gallery", json!({})).unwrap();
        assert_eq!(view["items"][0]["prompt"], "고양이");
        assert_eq!(view["items"][0]["status"], "missing");
        assert_eq!(view["folders"][0]["isWorkDir"], true);
        assert_eq!(view["truncated"], false);
        let image = work.join("ImageGallery").join("output").join("t").join("a.png");
        assert!(app.asset_protocol_scope().is_allowed(&image), "listing allows the work folder's gallery");
        assert_eq!(invoke(&webview, "gallery", json!({"limit": 0})).unwrap()["truncated"], true);

        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let outside = project.join("ImageGallery").join("output").join("p").join("b.png");
        assert!(!app.asset_protocol_scope().is_allowed(&outside));
        let saved = invoke(&webview, "add_gallery_folder", json!({ "path": project })).unwrap();
        assert_eq!(saved["galleryFolders"], json!([project]));
        assert!(app.asset_protocol_scope().is_allowed(&outside), "adding a folder allows its gallery");

        let err = invoke(&webview, "add_gallery_folder", json!({"path": dir.path().join("nowhere")})).unwrap_err();
        assert_eq!(err["category"], "not_found");
        let saved = invoke(&webview, "remove_gallery_folder", json!({ "path": project })).unwrap();
        assert_eq!(saved["galleryFolders"], json!([]));
        let err = invoke(&webview, "remove_gallery_folder", json!({ "path": work })).unwrap_err();
        assert_eq!(err["category"], "invalid_request");

        // A new work folder is allowed as soon as settings are saved, before any gallery call.
        let moved = dir.path().join("moved");
        let new_image = moved.join("ImageGallery").join("output").join("t").join("c.png");
        assert!(!app.asset_protocol_scope().is_allowed(&new_image));
        invoke(&webview, "save_settings", json!({"settings": {"workDir": moved, "galleryFolders": []}})).unwrap();
        assert!(app.asset_protocol_scope().is_allowed(&new_image), "saving settings allows the new work folder's gallery");
    }

    #[test]
    fn inspect_reference_command_allows_only_that_file_for_its_thumbnail() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (app, webview) = app(home.path(), Arc::new(MemoryStore::new()));
        let picked = dir.path().join("cup.png");
        let sibling = dir.path().join("other.png");
        std::fs::write(&picked, test_fixture::png_bytes(3, 2, [9, 9, 9, 255])).unwrap();
        std::fs::write(&sibling, test_fixture::png_bytes(3, 2, [9, 9, 9, 255])).unwrap();
        assert!(!app.asset_protocol_scope().is_allowed(&picked));

        let view = invoke(&webview, "inspect_reference", json!({ "path": picked })).unwrap();
        assert_eq!((view["width"].as_u64(), view["height"].as_u64(), view["name"].as_str()), (Some(3), Some(2), Some("cup.png")));
        assert!(app.asset_protocol_scope().is_allowed(&picked), "the picked reference shows as a thumbnail");
        assert!(!app.asset_protocol_scope().is_allowed(&sibling), "only the picked file, not its folder");

        let err = invoke(&webview, "inspect_reference", json!({"path": dir.path().join("none.png")})).unwrap_err();
        assert_eq!(err["category"], "invalid_request");
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
