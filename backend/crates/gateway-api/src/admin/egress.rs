//! Authenticated, write-only proxy secret administration.

use super::{
    AdminAuth, AdminEnvelope, AdminError, AdminJson, AdminResponse, AdminSessionState,
    wire::map_admin_service_error,
};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use gateway_admin::model::egress::EgressMutation;
use gateway_core::egress::ProxyEndpoint;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum MutationRequest {
    Save {
        id: Option<String>,
        name: String,
        endpoint: Option<String>,
        enabled: bool,
        #[serde(rename = "expectedRevision")]
        expected_revision: Option<u64>,
    },
    Delete {
        id: String,
        #[serde(rename = "expectedRevision")]
        expected_revision: u64,
    },
    Bind {
        #[serde(rename = "accountIds")]
        account_ids: Vec<String>,
        #[serde(rename = "proxyId")]
        proxy_id: Option<String>,
    },
}

impl MutationRequest {
    fn command(self) -> Result<EgressMutation, AdminError> {
        let invalid = || AdminError::bad_request("Invalid proxy configuration");
        Ok(match self {
            Self::Save {
                id,
                name,
                endpoint,
                enabled,
                expected_revision,
            } => {
                if id.is_some() != expected_revision.is_some() {
                    return Err(invalid());
                }
                let endpoint = endpoint
                    .map(|s| ProxyEndpoint::parse(&s))
                    .transpose()
                    .map_err(|_| invalid())?;
                if id.is_none() && endpoint.is_none() {
                    return Err(invalid());
                }
                EgressMutation::Save {
                    id: id.unwrap_or_else(|| format!("proxy_{}", Uuid::now_v7().simple())),
                    name,
                    endpoint,
                    enabled,
                    expected_revision,
                }
            }
            Self::Delete {
                id,
                expected_revision,
            } => EgressMutation::Delete {
                id,
                expected_revision,
            },
            Self::Bind {
                account_ids,
                proxy_id,
            } => EgressMutation::Bind {
                account_ids,
                proxy_id,
            },
        })
    }
}

pub fn router<S>() -> Router<S>
where
    S: AdminSessionState + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/admin/egress", get(list::<S>))
        .route("/api/admin/egress/update", post(update::<S>))
}

async fn list<S>(_auth: AdminAuth, State(state): State<S>) -> Result<impl IntoResponse, AdminError>
where
    S: AdminSessionState + Send + Sync,
{
    let directory = state
        .admin_services()
        .accounts()
        .egress_directory()
        .await
        .map_err(map_admin_service_error)?;
    let proxies: Vec<_> = directory
        .proxies
        .into_iter()
        .map(|p| {
            json!({
                "id":p.id,"name":p.name,"address":p.address,"enabled":p.enabled,
                "revision":p.revision,"accountCount":p.account_count,
            })
        })
        .collect();
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(json!({"proxies":proxies,"bindings":directory.bindings})),
    ))
}

async fn update<S>(
    auth: AdminAuth,
    State(state): State<S>,
    AdminJson(request): AdminJson<MutationRequest>,
) -> Result<impl IntoResponse, AdminError>
where
    S: AdminSessionState + Send + Sync,
{
    let result = state
        .admin_services()
        .accounts()
        .mutate_egress(&auth.context().mutation_context(), request.command()?)
        .await
        .map_err(map_admin_service_error)?;
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(json!({"configRevision":result.config_revision.get()})),
    ))
}
