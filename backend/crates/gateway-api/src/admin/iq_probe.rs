//! 降智检测（fork 自有功能）的管理接口：读写设置、批量查询账号最近结果

use crate::auth::SessionState;

use std::collections::HashSet;

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use gateway_admin::model::iq_probe::{
    IQ_PROBE_MAX_ROUND_ACCOUNTS, IqProbeQuestion, IqProbeRound, IqProbeSettings,
};
use serde::{Deserialize, Serialize};

use super::{AdminAuth, AdminEnvelope, AdminError, AdminJson, AdminQuery, AdminResponse};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateRequest {
    settings: SettingsRequest,
    revision: u64,
}

/// 请求体单独定义并拒绝未知字段；存库模型保持宽松以兼容回滚
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsRequest {
    enabled: bool,
    interval_minutes: u32,
    stagger_minutes: u32,
    recheck_delay_minutes: u32,
    breaker_threshold: u32,
    questions: Vec<QuestionRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuestionRequest {
    prompt: String,
    model: String,
    reasoning_effort: Option<String>,
    pass_keywords: Vec<String>,
    #[serde(default)]
    fail_keywords: Vec<String>,
    max_attempts: u32,
}

impl From<SettingsRequest> for IqProbeSettings {
    fn from(request: SettingsRequest) -> Self {
        Self {
            enabled: request.enabled,
            interval_minutes: request.interval_minutes,
            stagger_minutes: request.stagger_minutes,
            recheck_delay_minutes: request.recheck_delay_minutes,
            breaker_threshold: request.breaker_threshold,
            questions: request
                .questions
                .into_iter()
                .map(|question| IqProbeQuestion {
                    prompt: question.prompt,
                    model: question.model,
                    reasoning_effort: question.reasoning_effort,
                    pass_keywords: question.pass_keywords,
                    fail_keywords: question.fail_keywords,
                    max_attempts: question.max_attempts,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoundsQuery {
    account_ids: Option<String>,
}

#[derive(Serialize)]
struct RoundsView {
    items: Vec<IqProbeRound>,
}

pub fn router<S>() -> Router<S>
where
    S: SessionState + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/admin/iq-probe/settings", get(settings::<S>))
        .route("/api/admin/iq-probe/settings/update", post(update::<S>))
        .route("/api/admin/iq-probe/rounds", get(rounds::<S>))
}

fn map_error(error: gateway_admin::model::AdminError) -> AdminError {
    super::wire::map_admin_service_error(error)
}

/// 逗号分隔的账号 ID：去空白、去空项、按首次出现去重，数量需在 1～200 之间
fn parse_account_ids(raw: Option<&str>) -> Result<Vec<String>, AdminError> {
    let mut seen = HashSet::new();
    let ids: Vec<String> = raw
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty() && seen.insert(*id))
        .map(str::to_owned)
        .collect();
    if ids.is_empty() || ids.len() > IQ_PROBE_MAX_ROUND_ACCOUNTS {
        return Err(AdminError::bad_request(format!(
            "accountIds 需为 1 到 {IQ_PROBE_MAX_ROUND_ACCOUNTS} 个账号 ID"
        )));
    }
    Ok(ids)
}

async fn settings<S>(_: AdminAuth, State(state): State<S>) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    let view = state
        .admin_services()
        .iq_probe()
        .settings()
        .await
        .map_err(map_error)?;
    Ok(AdminResponse::new(StatusCode::OK, AdminEnvelope::ok(view)))
}

async fn update<S>(
    auth: AdminAuth,
    State(state): State<S>,
    AdminJson(request): AdminJson<UpdateRequest>,
) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    if request.revision > i64::MAX as u64 {
        return Err(AdminError::bad_request("降智检测设置版本不合法"));
    }
    let view = state
        .admin_services()
        .iq_probe()
        .update_settings(
            &auth.context().mutation_context(),
            request.settings.into(),
            request.revision,
        )
        .await
        .map_err(map_error)?;
    Ok(AdminResponse::new(StatusCode::OK, AdminEnvelope::ok(view)))
}

async fn rounds<S>(
    _: AdminAuth,
    State(state): State<S>,
    AdminQuery(query): AdminQuery<RoundsQuery>,
) -> Result<impl IntoResponse, AdminError>
where
    S: SessionState + Send + Sync,
{
    let account_ids = parse_account_ids(query.account_ids.as_deref())?;
    let items = state
        .admin_services()
        .iq_probe()
        .recent_rounds(&account_ids)
        .await
        .map_err(map_error)?;
    Ok(AdminResponse::new(
        StatusCode::OK,
        AdminEnvelope::ok(RoundsView { items }),
    ))
}
