//! Connection check (spec §6.2): server reachable → key accepted → model list → image models.
//! Never generates; "can really generate" comes only from the last real generation in `status.json`.

use crate::adapter::{list_models, model_supported};
use crate::catalog::is_listable_image_model;
use crate::connection::Connection;
use crate::error::{Category, EngineError};
use crate::http::Http;
use crate::profiles::{transport_warning, GenerationStatus, ProfileStore, ProviderKind};
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct ConnectionReport {
    /// Profile name, or `None` for an environment connection.
    pub profile: Option<String>,
    pub kind: ProviderKind,
    pub base_url: String,
    pub key_configured: bool,
    pub reachable: bool,
    pub authenticated: bool,
    pub models_listed: bool,
    /// Listed models this connection can generate with, in list order.
    pub image_models: Vec<String>,
    pub last_generation: Option<GenerationStatus>,
    pub warning: Option<String>,
    pub error: Option<EngineError>,
}

pub async fn check_connection(http: &Http, conn: &Connection, store: Option<&ProfileStore>) -> ConnectionReport {
    let last_generation = match (store, conn.profile_id.as_deref()) {
        (Some(store), Some(id)) => store.generation_status().ok().and_then(|mut all| all.remove(id)),
        _ => None,
    };
    let mut report = ConnectionReport {
        profile: conn.source.strip_prefix("profile:").map(str::to_string),
        kind: conn.kind,
        base_url: conn.base_url.clone(),
        key_configured: conn.api_key.is_some(),
        reachable: false,
        authenticated: false,
        models_listed: false,
        image_models: Vec::new(),
        last_generation,
        warning: transport_warning(&conn.base_url),
        error: None,
    };
    match list_models(http, conn).await {
        Ok(ids) => {
            report.reachable = true;
            report.authenticated = true;
            report.models_listed = true;
            for id in ids {
                if is_listable_image_model(&id) && model_supported(conn.kind, &id).is_ok() && !report.image_models.contains(&id) {
                    report.image_models.push(id);
                }
            }
        }
        Err(err) => {
            // Any HTTP answer (redirects included) means the server was reached.
            report.reachable = err.http_status.is_some();
            // A quota or rate-limit answer is given only after the key was accepted.
            report.authenticated = report.reachable && matches!(err.category, Category::Quota | Category::RateLimit);
            report.error = Some(err);
        }
    }
    report
}
