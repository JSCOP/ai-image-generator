//! Loopback fake image provider for tests. Never calls a real provider.

use base64::Engine;
use image_core::connection::EnvSource;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub delay: Duration,
}

impl Reply {
    pub fn json(status: u16, body: &str) -> Reply {
        Reply {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.as_bytes().to_vec(),
            delay: Duration::ZERO,
        }
    }

    pub fn redirect(status: u16, location: &str) -> Reply {
        Reply { status, headers: vec![("Location".into(), location.into())], body: Vec::new(), delay: Duration::ZERO }
    }

    pub fn delayed(mut self, delay: Duration) -> Reply {
        self.delay = delay;
        self
    }
}

type Handler = dyn Fn(&Recorded) -> Reply + Send + Sync;

pub struct FakeProvider {
    server: Arc<tiny_http::Server>,
    port: u16,
    requests: Arc<Mutex<Vec<Recorded>>>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

impl FakeProvider {
    pub fn start(handler: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> FakeProvider {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").expect("bind fake provider"));
        let port = server.server_addr().to_ip().expect("ip address").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let handler: Arc<Handler> = Arc::new(handler);
        let (srv, reqs, act, pk) = (server.clone(), requests.clone(), active.clone(), peak.clone());
        std::thread::spawn(move || {
            for mut request in srv.incoming_requests() {
                let (handler, reqs, act, pk) = (handler.clone(), reqs.clone(), act.clone(), pk.clone());
                std::thread::spawn(move || {
                    let now = act.fetch_add(1, Ordering::SeqCst) + 1;
                    pk.fetch_max(now, Ordering::SeqCst);
                    let mut body = Vec::new();
                    let _ = request.as_reader().read_to_end(&mut body);
                    let recorded = Recorded {
                        method: request.method().to_string(),
                        path: request.url().to_string(),
                        headers: request.headers().iter().map(|h| (h.field.to_string(), h.value.to_string())).collect(),
                        body,
                    };
                    reqs.lock().unwrap().push(recorded.clone());
                    let reply = handler(&recorded);
                    std::thread::sleep(reply.delay);
                    let mut response = tiny_http::Response::new(
                        tiny_http::StatusCode(reply.status),
                        Vec::new(),
                        Cursor::new(reply.body.clone()),
                        Some(reply.body.len()),
                        None,
                    );
                    for (k, v) in &reply.headers {
                        response.add_header(tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
                    }
                    let _ = request.respond(response);
                    act.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        FakeProvider { server, port, requests, active, peak }
    }

    /// `http://127.0.0.1:<port>/v1`
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// Highest number of requests in flight at the same time.
    pub fn peak_concurrency(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    pub fn in_flight(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

/// A port with nothing listening.
pub fn closed_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v1")
}

pub fn png_bytes(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba(rgba));
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn openai_image_reply(png: &[u8]) -> Reply {
    Reply::json(200, &serde_json::json!({"model": "gpt-image-test", "data": [{"b64_json": b64(png)}]}).to_string())
}

pub fn gemini_image_reply(png: &[u8], model: &str) -> Reply {
    let body = serde_json::json!({
        "modelVersion": model,
        "candidates": [{"content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": b64(png)}}]}}]
    });
    Reply::json(200, &body.to_string())
}

/// In-memory environment for connection tests.
pub struct MapEnv {
    process: HashMap<String, String>,
    user: HashMap<String, String>,
}

impl MapEnv {
    pub fn new(process: &[(&str, &str)], user: &[(&str, &str)]) -> MapEnv {
        let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        MapEnv { process: map(process), user: map(user) }
    }

    /// Process env pointing at `base_url` with a fake key.
    pub fn proxy(base_url: &str) -> MapEnv {
        MapEnv::new(&[("CLIPROXY_BASE_URL", base_url), ("CLIPROXY_API_KEY", "fake-token")], &[])
    }
}

impl EnvSource for MapEnv {
    fn process(&self, key: &str) -> Option<String> {
        self.process.get(key).cloned()
    }

    fn user(&self, key: &str) -> Option<String> {
        self.user.get(key).cloned()
    }
}
