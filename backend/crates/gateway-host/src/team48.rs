//! 同机 48team 邮箱自动授权内部接口的 HTTP 适配
//!
//! 固定禁用环境代理与重定向，限制超时与响应体大小，日志不记录密钥、邮箱或响应原文

use std::{fmt, time::Duration};

use async_trait::async_trait;
use futures::StreamExt as _;
use gateway_admin::{
    ForkTeam48Config,
    ports::team48::{Team48BusinessStatus, Team48Error, Team48Onboard},
};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Serialize;
use serde_json::Value;

use crate::ConfigError;

const ONBOARD_PATH: &str = "/api/internal/codex-rs/onboard";
const STOP_PATH: &str = "/api/internal/codex-rs/onboard/stop";
const RETRY_PATH: &str = "/api/internal/codex-rs/onboard/retry";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_ERROR_MESSAGE_CHARS: usize = 512;

/// 直连同机 Docker 内网中 48team 的 HTTP 客户端
pub struct HttpTeam48Onboard {
    client: reqwest::Client,
    base_url: String,
    api_key: SecretString,
}

impl fmt::Debug for HttpTeam48Onboard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpTeam48Onboard")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Serialize)]
struct SubmitPayload {
    emails: Vec<String>,
    settings: Value,
}

#[derive(Serialize)]
struct RetryPayload {
    email: String,
}

#[derive(Serialize)]
struct EmptyPayload {}

#[derive(Clone, Copy)]
struct CallSpec {
    path: &'static str,
    allow_null_batch: bool,
    allow_business_not_found: bool,
}

impl HttpTeam48Onboard {
    /// 构造禁用环境代理与重定向的 48team 客户端
    pub fn new(mut config: ForkTeam48Config) -> Result<Self, ConfigError> {
        config
            .resolve_and_validate()
            .map_err(|_| ConfigError::InvalidField("fork_team48"))?;
        // 容器内可能配置 HTTP(S)_PROXY 供外网使用，同机内网通信必须绕过环境代理
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ConfigError::InvalidField("fork_team48"))?;
        Ok(Self {
            client,
            base_url: config.base_url,
            api_key: config.api_key,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn execute(
        &self,
        spec: CallSpec,
        request: reqwest::RequestBuilder,
    ) -> Result<Value, Team48Error> {
        let request = request
            .bearer_auth(self.api_key.expose_secret())
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(REQUEST_TIMEOUT);

        let (status, body) = match tokio::time::timeout(REQUEST_TIMEOUT, async {
            let response = request.send().await.map_err(classify_transport_error)?;
            let status = response.status();
            let bytes = read_bounded_body(response, spec.path).await?;
            Ok::<_, Team48Error>((status, bytes))
        })
        .await
        {
            Ok(Ok(pair)) => pair,
            Ok(Err(error)) => {
                if matches!(error, Team48Error::Unreachable) {
                    tracing::warn!(path = spec.path, kind = "unreachable", "请求 48team 失败");
                }
                return Err(error);
            }
            Err(_) => {
                tracing::warn!(path = spec.path, kind = "timeout", "请求 48team 超时");
                return Err(Team48Error::Unreachable);
            }
        };

        if status.is_success() {
            return parse_batch_response(&body, status.as_u16(), spec);
        }

        Err(classify_error_status(status.as_u16(), &body, spec))
    }
}

#[async_trait]
impl Team48Onboard for HttpTeam48Onboard {
    async fn get(&self) -> Result<Value, Team48Error> {
        let spec = CallSpec {
            path: ONBOARD_PATH,
            allow_null_batch: true,
            allow_business_not_found: false,
        };
        self.execute(spec, self.client.get(self.endpoint(ONBOARD_PATH)))
            .await
    }

    async fn submit(&self, emails: Vec<String>, settings: Value) -> Result<Value, Team48Error> {
        let spec = CallSpec {
            path: ONBOARD_PATH,
            allow_null_batch: false,
            allow_business_not_found: false,
        };
        let payload = SubmitPayload { emails, settings };
        self.execute(
            spec,
            self.client.post(self.endpoint(ONBOARD_PATH)).json(&payload),
        )
        .await
    }

    async fn stop(&self) -> Result<Value, Team48Error> {
        let spec = CallSpec {
            path: STOP_PATH,
            allow_null_batch: false,
            allow_business_not_found: true,
        };
        self.execute(
            spec,
            self.client
                .post(self.endpoint(STOP_PATH))
                .json(&EmptyPayload {}),
        )
        .await
    }

    async fn retry(&self, email: String) -> Result<Value, Team48Error> {
        let spec = CallSpec {
            path: RETRY_PATH,
            allow_null_batch: false,
            allow_business_not_found: true,
        };
        let payload = RetryPayload { email };
        self.execute(
            spec,
            self.client.post(self.endpoint(RETRY_PATH)).json(&payload),
        )
        .await
    }
}

fn classify_transport_error(_error: reqwest::Error) -> Team48Error {
    Team48Error::Unreachable
}

async fn read_bounded_body(
    response: reqwest::Response,
    path: &'static str,
) -> Result<Vec<u8>, Team48Error> {
    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        tracing::warn!(
            path,
            status = response.status().as_u16(),
            kind = "body_too_large",
            "48team 响应体超过上限"
        );
        return Err(Team48Error::InvalidResponse);
    }

    let status_code = response.status().as_u16();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| {
            if err.is_timeout() || err.is_connect() {
                Team48Error::Unreachable
            } else {
                tracing::warn!(
                    path,
                    status = status_code,
                    kind = "body_read_error",
                    "读取 48team 响应体失败"
                );
                Team48Error::InvalidResponse
            }
        })?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            tracing::warn!(
                path,
                status = status_code,
                kind = "body_too_large",
                "48team 响应体超过上限"
            );
            return Err(Team48Error::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn parse_batch_response(bytes: &[u8], status: u16, spec: CallSpec) -> Result<Value, Team48Error> {
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(bytes) else {
        tracing::warn!(
            path = spec.path,
            status,
            kind = "invalid_json",
            "48team 成功响应不是 JSON 对象"
        );
        return Err(Team48Error::InvalidResponse);
    };
    let Some(batch) = map.remove("batch") else {
        tracing::warn!(
            path = spec.path,
            status,
            kind = "missing_batch",
            "48team 响应缺少 batch 字段"
        );
        return Err(Team48Error::InvalidResponse);
    };
    if batch.is_object() || (spec.allow_null_batch && batch.is_null()) {
        Ok(batch)
    } else {
        tracing::warn!(
            path = spec.path,
            status,
            kind = "invalid_batch",
            "48team 响应的 batch 字段类型不合法"
        );
        Err(Team48Error::InvalidResponse)
    }
}

fn classify_error_status(status: u16, bytes: &[u8], spec: CallSpec) -> Team48Error {
    if status == 401 {
        tracing::warn!(
            path = spec.path,
            status,
            kind = "auth_failed",
            "48team 拒绝内部密钥"
        );
        return Team48Error::AuthFailed;
    }

    let parsed = parse_error_fields(bytes);

    if status == 404 {
        // 48team 未配置内部密钥时统一返回 404；仅 stop / retry 带业务错误码时才是资源不存在
        if let Some((code, message)) = parsed
            && spec.allow_business_not_found
            && !matches!(
                code.as_str(),
                "not_found" | "unconfigured" | "not_configured" | "disabled"
            )
        {
            let message = sanitize_message(&message, "目标不存在");
            tracing::warn!(
                path = spec.path,
                status,
                kind = "business_error",
                "48team 返回业务未找到错误"
            );
            return Team48Error::Business {
                status: Team48BusinessStatus::NotFound,
                code,
                message,
            };
        }
        tracing::warn!(
            path = spec.path,
            status,
            kind = "auth_failed",
            "48team 返回未配置密钥的 404"
        );
        return Team48Error::AuthFailed;
    }

    let (business_status, fallback_message) = match status {
        400 => (Team48BusinessStatus::BadRequest, "请求参数不合法"),
        409 => (Team48BusinessStatus::Conflict, "当前状态冲突"),
        503 => (
            Team48BusinessStatus::ServiceUnavailable,
            "48team 服务暂不可用",
        ),
        _ => {
            tracing::warn!(
                path = spec.path,
                status,
                kind = "unexpected_status",
                "48team 返回非预期状态码"
            );
            return Team48Error::InvalidResponse;
        }
    };

    let (code, raw_message) = parsed.unwrap_or_default();
    let message = sanitize_message(&raw_message, fallback_message);
    tracing::warn!(
        path = spec.path,
        status,
        kind = "business_error",
        "48team 返回业务错误"
    );
    Team48Error::Business {
        status: business_status,
        code,
        message,
    }
}

/// 从 `{"code", "message"}` 或 FastAPI `{"detail": {"code", "message"}}` 中提取错误字段
fn parse_error_fields(bytes: &[u8]) -> Option<(String, String)> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let object = value
        .as_object()
        .filter(|map| map.contains_key("code") || map.contains_key("message"))
        .or_else(|| value.get("detail").and_then(Value::as_object))?;
    let code = object
        .get("code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    Some((code.to_owned(), message.to_owned()))
}

fn sanitize_message(raw: &str, fallback: &'static str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_ERROR_MESSAGE_CHARS
        || trimmed.chars().any(char::is_control)
    {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}
