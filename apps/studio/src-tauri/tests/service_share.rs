//! Galleries found on this PC (gallery-index.json, "이 PC에서 찾기") and other PCs ("이 PC 공유" / "다른 PC"),
//! over temp folders and a share server on loopback.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use image_core::secrets::MemoryStore;
use serde_json::{json, Value};
use studio_lib::service::gallery::{add_folder, gallery_with, remove_folder, FolderSource};
use studio_lib::service::index::{find_galleries, load_index, record_roots};
use studio_lib::service::peers::{self, normalize_url, parse_request, split_remote};
use studio_lib::service::settings::{load_settings, save_settings, Settings};
use studio_lib::service::share::{self, allowed_peer};
use studio_lib::service::Ctx;
use test_fixture::MapEnv;
use tokio_util::sync::CancellationToken;

/// `<workspace>/ImageGallery/{metadata,output}/<topic>/<name>` with a 4x3 PNG.
fn put(workspace: &Path, topic: &str, name: &str, prompt: &str) -> PathBuf {
    let root = workspace.join("ImageGallery");
    let image = root.join("output").join(topic).join(name);
    let record = root.join("metadata").join(topic).join(format!("{name}.json"));
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&image, test_fixture::png_bytes(4, 3, [200, 10, 10, 255])).unwrap();
    let rec = json!({"prompt": prompt, "image_model": "gpt-image-2.5", "size": "4x3", "status": "complete", "output": image});
    std::fs::write(&record, rec.to_string()).unwrap();
    image
}

fn ctx(home: &Path) -> Ctx {
    Ctx { home: home.to_path_buf(), env: Arc::new(MapEnv::new(&[], &[])), secrets: Arc::new(MemoryStore::new()) }
}

#[test]
fn find_galleries_finds_nested_galleries_and_skips_build_folders_and_links() {
    let dir = tempfile::tempdir().unwrap();
    put(&dir.path().join("org").join("project"), "t", "a.png", "a");
    put(&dir.path().join("node_modules").join("pkg"), "t", "b.png", "b");
    put(&dir.path().join(".hidden"), "t", "c.png", "c");
    // An ImageGallery without metadata is not a gallery.
    std::fs::create_dir_all(dir.path().join("empty").join("ImageGallery").join("output")).unwrap();
    let found = find_galleries(&[dir.path().to_path_buf()], 8, 10_000, Duration::from_secs(30));
    assert_eq!(found, vec![dir.path().join("org").join("project").join("ImageGallery")]);
    // Too shallow a walk misses it.
    assert!(find_galleries(&[dir.path().to_path_buf()], 2, 10_000, Duration::from_secs(30)).is_empty());
}

#[test]
fn found_galleries_join_the_list_and_removing_one_hides_it() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let project = dir.path().join("project");
    put(&project, "cctv", "a.png", "탑승교");
    save_settings(home.path(), &Settings { work_dir: work.clone(), ..Settings::default() }).unwrap();

    // A CLI run records its topic_root (any folder inside the gallery maps to its root).
    let index = record_roots(home.path(), &[project.join("ImageGallery").join("output")], false).unwrap();
    assert_eq!(index.roots.len(), 1);
    assert_eq!(index.scanned_at, None);
    let view = gallery_with(&load_settings(home.path()), &load_index(home.path()), 100);
    let found = view.folders.iter().find(|f| f.source == FolderSource::Found).expect("found gallery listed");
    assert_eq!(PathBuf::from(&found.path), project, "shown by its project folder, not ImageGallery");
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].prompt, "탑승교");

    // The same root again changes nothing but the time.
    assert_eq!(record_roots(home.path(), std::slice::from_ref(&project), true).unwrap().roots.len(), 1);
    assert!(load_index(home.path()).scanned_at.is_some());

    remove_folder(home.path(), &project).unwrap();
    let view = gallery_with(&load_settings(home.path()), &load_index(home.path()), 100);
    assert!(view.items.is_empty(), "hidden after removing");
    assert!(view.folders.iter().all(|f| f.source != FolderSource::Found));

    add_folder(home.path(), &project).unwrap();
    let settings = load_settings(home.path());
    assert!(settings.hidden_folders.is_empty(), "adding it again unhides it");
    assert_eq!(gallery_with(&settings, &load_index(home.path()), 100).items.len(), 1);
}

#[test]
fn only_loopback_and_tailscale_addresses_may_connect() {
    for ip in ["127.0.0.1", "100.64.0.1", "100.100.12.34", "100.127.255.255", "::1", "fd7a:115c:a1e0::1"] {
        assert!(allowed_peer(ip.parse::<IpAddr>().unwrap()), "{ip}");
    }
    for ip in ["192.168.0.10", "10.0.0.2", "100.63.255.255", "100.128.0.1", "8.8.8.8", "fe80::1"] {
        assert!(!allowed_peer(ip.parse::<IpAddr>().unwrap()), "{ip}");
    }
}

#[test]
fn peer_urls_and_image_requests_parse() {
    assert_eq!(normalize_url("100.90.1.2").unwrap(), "http://100.90.1.2:8329");
    assert_eq!(normalize_url(" http://js:9000/ ").unwrap(), "http://js:9000");
    assert_eq!(normalize_url("js").unwrap(), "http://js:8329");
    assert!(normalize_url("").is_err());
    assert!(normalize_url("ftp://x").is_err());

    let request = parse_request("/t%2Fpeer-1234abcd%2FC%3A%5Cw%5CImageGallery%5Coutput%5Ct%5Ca%2Bb.png").unwrap();
    assert!(request.thumb);
    assert_eq!(request.id, "peer-1234abcd");
    assert_eq!(request.path, "C:\\w\\ImageGallery\\output\\t\\a+b.png");
    assert!(parse_request("/x/peer-1/C:/a.png").is_none());
    assert_eq!(split_remote("peer://peer-1/C:\\a.png"), Some(("peer-1", "C:\\a.png")));
}

async fn get(url: &str, token: &str) -> (u16, Vec<u8>, String) {
    let response = reqwest::Client::new().get(url).bearer_auth(token).send().await.unwrap();
    let status = response.status().as_u16();
    let kind = response.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    (status, response.bytes().await.unwrap().to_vec(), kind)
}

#[tokio::test]
async fn share_server_serves_only_listed_gallery_images_with_the_token_and_a_peer_reads_it() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let image = put(&work, "t", "a.png", "원격 고양이");
    let outside = dir.path().join("secret.png");
    std::fs::write(&outside, test_fixture::png_bytes(2, 2, [0, 0, 0, 255])).unwrap();
    save_settings(home.path(), &Settings { work_dir: work.clone(), ..Settings::default() }).unwrap();

    let server = ctx(home.path());
    let token = share::share_token(&server).unwrap();
    assert_eq!(share::share_token(&server).unwrap(), token, "made once, then kept");
    let listener = share::bind(0).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = CancellationToken::new();
    tokio::spawn(share::serve(listener, home.path().to_path_buf(), token.clone(), stop.clone()));
    let base = format!("http://127.0.0.1:{port}");

    assert_eq!(get(&format!("{base}/v1/info"), "wrong").await.0, 401);
    let (status, body, _) = get(&format!("{base}/v1/info"), &token).await;
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["version"], env!("CARGO_PKG_VERSION"));

    let (status, body, _) = get(&format!("{base}/v1/gallery?limit=10"), &token).await;
    assert_eq!(status, 200);
    let view: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(view["items"][0]["prompt"], "원격 고양이");

    let q = |path: &Path| url::form_urlencoded::Serializer::new(String::new()).append_pair("path", &path.to_string_lossy()).finish();
    let (status, body, kind) = get(&format!("{base}/v1/image?{}", q(&image)), &token).await;
    assert_eq!((status, kind.as_str()), (200, "image/png"));
    assert_eq!(body, std::fs::read(&image).unwrap());
    let (status, _, kind) = get(&format!("{base}/v1/image?{}&w=64", q(&image)), &token).await;
    assert_eq!((status, kind.as_str()), (200, "image/jpeg"), "thumbnail of an opaque image");
    assert_eq!(get(&format!("{base}/v1/image?{}", q(&outside)), &token).await.0, 403, "nothing outside the galleries");
    let sneaky = work.join("ImageGallery").join("..").join("..").join("secret.png");
    assert_eq!(get(&format!("{base}/v1/image?{}", q(&sneaky)), &token).await.0, 403, "no way out with ..");

    // The other PC: checks, saves, lists with peer:// paths, and fetches a thumbnail (then from its cache).
    let client_home = tempfile::tempdir().unwrap();
    let client = ctx(client_home.path());
    save_settings(client_home.path(), &Settings { work_dir: dir.path().join("client-work"), ..Settings::default() }).unwrap();
    assert!(peers::add_peer(&client, "", &base, "wrong").await.is_err(), "a wrong token is not saved");
    let settings = peers::add_peer(&client, "", &base, &token).await.unwrap();
    let peer = settings.peers[0].clone();
    assert_eq!(peer.name, share::machine_name(), "named after that PC");
    let remote = peers::peer_gallery(&client, &peer, 10).await.unwrap();
    let item = &remote.items[0];
    assert_eq!(item.machine.as_deref(), Some(peer.name.as_str()));
    let (id, there) = split_remote(&item.image_path).unwrap();
    assert_eq!((id, PathBuf::from(there)), (peer.id.as_str(), image.clone()));
    assert!(remote.folders.iter().all(|f| f.source == FolderSource::Peer && f.gallery_root.starts_with("peer://")));

    let request = peers::ImageRequest { thumb: true, id: peer.id.clone(), path: there.to_string() };
    let (thumb, kind) = peers::peer_image(&client, &request).await.unwrap();
    assert_eq!(kind, "image/jpeg");
    stop.cancel();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(peers::peer_image(&client, &request).await.unwrap().0, thumb, "served from the cache once fetched");

    // A PC that is off is one unavailable folder, not an error of the whole gallery.
    let error = peers::peer_gallery(&client, &peer, 10).await.unwrap_err();
    let folder = peers::unreachable(&peer, &error);
    assert!(!folder.available && folder.note.is_some());

    peers::remove_peer(&client, &peer.id).unwrap();
    assert!(load_settings(client_home.path()).peers.is_empty());
    assert!(peers::cache_dir().is_none_or(|dir| !dir.join(&peer.id).exists()), "its cached images go too");
}
