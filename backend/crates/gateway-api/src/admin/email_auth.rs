//! 邮箱自动授权（fork 自有功能）的管理端路由：将批次查询、提交、停止与单号重试转发给 48team

use crate::auth::SessionState;

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use gateway_admin::TEAM48_NOT_CONFIGURED_MESSAGE;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{AdminAuth, AdminEnvelope, AdminError, AdminJson, AdminResponse};

const MAX_SUBMIT_EMAILS: usize = 50;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmitRequest {
    emails: Vec<String>,
    settings: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StopRequest {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetryRequest {
    email: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailAuthView {
    configured: bool,
    batch: Value,
}

pub fn router<S>() -> Router<S>
where
    S: SessionState + Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/admin/accounts/email-auth",
            get(get_status::<S>).post(submit::<S>),
        )
        .route("/api/admin/accounts/email-auth/stop", post(stop::<S>))
        .route("/api/admin/accounts/email-auth/retry", post(retry::<S>))
}

fn map_error(error: gateway_admin::model::AdminError) -> AdminError {
    super::wire::map_admin_service_error(error)
}

fn not_configured_error() -> AdminError {
    map_error(gateway_admin::model::AdminError::unavailable(
        TEAM48_NOT_CONFIGURED_MESSAGE,
    ))
}

async fn get_status<S>(
    _: AdminAuth,
    State(state): State<S>,
) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    let service = state.admin_services().email_auth();
    if !service.configured() {
        return Ok(AdminResponse::new(
            StatusCode::OK,
            AdminEnvelope::ok(EmailAuthView {
                configured: false,
                batch: Value::Null,
            }),
        ));
    }
    let batch = service.get().await.map_err(map_error)?;
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(EmailAuthView {
            configured: true,
            batch,
        }),
    ))
}

async fn submit<S>(
    _: AdminAuth,
    State(state): State<S>,
    AdminJson(request): AdminJson<SubmitRequest>,
) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    let service = state.admin_services().email_auth();
    if !service.configured() {
        return Err(not_configured_error());
    }
    if request.emails.is_empty() || request.emails.len() > MAX_SUBMIT_EMAILS {
        return Err(AdminError::bad_request(format!(
            "每次需提交 1 到 {MAX_SUBMIT_EMAILS} 个邮箱"
        )));
    }
    if !request.settings.is_object() {
        return Err(AdminError::bad_request("settings 必须是 JSON 对象"));
    }
    let batch = service
        .submit(request.emails, request.settings)
        .await
        .map_err(map_error)?;
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(EmailAuthView {
            configured: true,
            batch,
        }),
    ))
}

async fn stop<S>(
    _: AdminAuth,
    State(state): State<S>,
    _: Option<AdminJson<StopRequest>>,
) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    let batch = state
        .admin_services()
        .email_auth()
        .stop()
        .await
        .map_err(map_error)?;
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(EmailAuthView {
            configured: true,
            batch,
        }),
    ))
}

async fn retry<S>(
    _: AdminAuth,
    State(state): State<S>,
    AdminJson(request): AdminJson<RetryRequest>,
) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    let service = state.admin_services().email_auth();
    if !service.configured() {
        return Err(not_configured_error());
    }
    if request.email.trim().is_empty() {
        return Err(AdminError::bad_request("email 不能为空"));
    }
    let batch = service.retry(request.email).await.map_err(map_error)?;
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(EmailAuthView {
            configured: true,
            batch,
        }),
    ))
}
