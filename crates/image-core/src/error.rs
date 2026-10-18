//! Structured engine errors (spec §5.4). Messages never carry credentials.

use crate::connection::Secret;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Network,
    Dns,
    Timeout,
    Tls,
    Auth,
    Permission,
    Quota,
    RateLimit,
    Conflict,
    Cancelled,
    NotFound,
    Redirect,
    Unsupported,
    InvalidRequest,
    Provider,
    Internal,
}

impl Category {
    pub fn retryable_by_default(self) -> bool {
        matches!(self, Category::Network | Category::Dns | Category::Timeout | Category::RateLimit | Category::Provider)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct EngineError {
    pub category: Category,
    pub provider_code: Option<String>,
    pub http_status: Option<u16>,
    pub retryable: bool,
    pub message: String,
}

impl EngineError {
    pub fn new(category: Category, message: impl Into<String>) -> Self {
        EngineError {
            category,
            provider_code: None,
            http_status: None,
            retryable: category.retryable_by_default(),
            message: message.into(),
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// One line of at most 300 characters for the result JSON `reason`.
    pub fn reason(&self) -> String {
        let line = self.message.split_whitespace().collect::<Vec<_>>().join(" ");
        line.chars().take(300).collect()
    }
}

static BEARER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(bearer\s+)\S+").unwrap());

/// Remove the API key value and any bearer token from text that may be shown or stored.
pub fn redact(text: &str, secret: Option<&Secret>) -> String {
    let mut out = text.to_string();
    if let Some(secret) = secret {
        if !secret.expose().is_empty() {
            out = out.replace(secret.expose(), "***");
        }
    }
    BEARER.replace_all(&out, "${1}***").into_owned()
}
