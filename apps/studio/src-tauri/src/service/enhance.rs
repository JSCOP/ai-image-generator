//! Prompt enhancement ("영어로 다듬기"): one chat call through the chosen connection; nothing is saved.

use std::time::Duration;

use image_core::enhance::{self, EnhanceRequest};
use image_core::http::Http;
use serde::{Deserialize, Serialize};

use super::profiles::{connection, store};
use super::{Ctx, ServiceError};

const ENHANCE_TIMEOUT: Duration = Duration::from_secs(90);
const LIST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnhanceInput {
    /// Profile name; `None` = the active connection.
    #[serde(default)]
    pub profile: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub avoid: String,
    #[serde(default)]
    pub image_model: Option<String>,
    /// Chat model that writes the prompt.
    pub model: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnhanceView {
    pub prompt: String,
    /// Model the provider reported, else the requested one.
    pub model: String,
}

pub async fn enhance(ctx: &Ctx, input: EnhanceInput) -> Result<EnhanceView, ServiceError> {
    let conn = connection(ctx, &store(ctx), input.profile.as_deref())?;
    let req =
        EnhanceRequest { prompt: &input.prompt, avoid: &input.avoid, image_model: input.image_model.as_deref(), text_model: &input.model };
    let out = enhance::enhance_prompt(&Http::new(Some(ENHANCE_TIMEOUT)), &conn, &req)
        .await
        .map_err(|e| ServiceError::from_engine(e, conn.api_key.as_ref()))?;
    Ok(EnhanceView { prompt: out.prompt, model: out.response_model.unwrap_or_else(|| input.model.trim().to_string()) })
}

/// Chat models of the connection (no paid request).
pub async fn text_models(ctx: &Ctx, name: Option<&str>) -> Result<Vec<String>, ServiceError> {
    let conn = connection(ctx, &store(ctx), name)?;
    enhance::text_models(&Http::new(Some(LIST_TIMEOUT)), &conn).await.map_err(|e| ServiceError::from_engine(e, conn.api_key.as_ref()))
}
