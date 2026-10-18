//! The one HTTP client for model listing, generation and edits. Redirects are never followed.

use crate::connection::Secret;
use crate::error::{Category, EngineError};
use serde_json::Value;
use std::error::Error as _;
use std::time::Duration;

const MAX_BODY: usize = 64 * 1024 * 1024;

pub struct Http {
    client: reqwest::Client,
}

impl Http {
    pub fn new(timeout: Option<Duration>) -> Http {
        let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        Http { client: builder.build().expect("HTTP client") }
    }

    pub async fn post_json(&self, url: &str, body: &Value, key: Option<&Secret>) -> Result<(u16, Vec<u8>), EngineError> {
        let request = self.client.post(url).header("Accept", "application/json").json(body);
        self.send(with_key(request, key)).await
    }

    pub async fn get(&self, url: &str, key: Option<&Secret>, extra: &[(&str, String)]) -> Result<(u16, Vec<u8>), EngineError> {
        let mut request = self.client.get(url).header("Accept", "application/json");
        for (name, value) in extra {
            request = request.header(*name, value);
        }
        self.send(with_key(request, key)).await
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<(u16, Vec<u8>), EngineError> {
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            return Err(EngineError::new(
                Category::Redirect,
                format!("HTTP {status}: the server asked to move to another address; not following it so the API key stays on the configured host. Check the base URL."),
            )
            .with_status(status));
        }
        let mut body = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(EngineError::new(Category::Provider, "provider response exceeds 64 MiB").with_status(status));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, body))
    }
}

fn with_key(request: reqwest::RequestBuilder, key: Option<&Secret>) -> reqwest::RequestBuilder {
    match key {
        Some(key) => request.bearer_auth(key.expose()),
        None => request,
    }
}

fn transport_error(err: reqwest::Error) -> EngineError {
    let mut chain = String::new();
    let mut source = err.source();
    while let Some(cause) = source {
        chain.push_str(&cause.to_string().to_lowercase());
        chain.push(' ');
        source = cause.source();
    }
    let category = if err.is_timeout() {
        Category::Timeout
    } else if chain.contains("dns") || chain.contains("resolve") || chain.contains("no such host") {
        Category::Dns
    } else if chain.contains("certificate") || chain.contains("tls") {
        Category::Tls
    } else {
        Category::Network
    };
    // reqwest's Display includes the URL but never headers, so no key can leak here.
    let detail = chain.trim().chars().take(200).collect::<String>();
    EngineError::new(category, format!("{err}: {detail}"))
}
