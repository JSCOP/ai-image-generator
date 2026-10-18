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

    pub async fn post_json(&self, url: &str, body: &Value, auth: Auth<'_>) -> Result<(u16, Vec<u8>), EngineError> {
        let request = self.client.post(url).header("Accept", "application/json").json(body);
        self.send(auth.apply(request)).await
    }

    pub async fn get(&self, url: &str, auth: Auth<'_>) -> Result<(u16, Vec<u8>), EngineError> {
        let request = self.client.get(url).header("Accept", "application/json");
        self.send(auth.apply(request)).await
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

/// How the key travels: `Authorization: Bearer` (OpenAI style) or `x-goog-api-key` (Gemini official).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthScheme {
    Bearer,
    GoogApiKey,
}

/// Key plus header scheme for one request. No key means no auth header.
#[derive(Clone, Copy, Debug)]
pub struct Auth<'a> {
    pub scheme: AuthScheme,
    pub key: Option<&'a Secret>,
}

impl<'a> Auth<'a> {
    pub const NONE: Auth<'static> = Auth { scheme: AuthScheme::Bearer, key: None };

    pub fn bearer(key: Option<&'a Secret>) -> Auth<'a> {
        Auth { scheme: AuthScheme::Bearer, key }
    }

    fn apply(self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match (self.scheme, self.key) {
            (_, None) => request,
            (AuthScheme::Bearer, Some(key)) => request.bearer_auth(key.expose()),
            (AuthScheme::GoogApiKey, Some(key)) => match reqwest::header::HeaderValue::from_str(key.expose()) {
                Ok(mut value) => {
                    value.set_sensitive(true);
                    request.header("x-goog-api-key", value)
                }
                // Same outcome as an unusable Bearer key: reqwest reports a builder error without the value.
                Err(_) => request.header("x-goog-api-key", key.expose()),
            },
        }
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
