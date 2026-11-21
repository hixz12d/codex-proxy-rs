//! 邮箱自动授权（fork 自有功能）的薄转发服务：持有可选 48team 端口并映射管理层错误

use std::sync::Arc;

use crate::model::AdminError;
use crate::ports::team48::{Team48BusinessStatus, Team48Error, Team48Onboard};

pub const TEAM48_NOT_CONFIGURED_MESSAGE: &str = "服务器未配置 48team 连接";

/// 供管理 HTTP 接口调用的邮箱自动授权服务；rs 本身不落库批次状态，原样透传 48team 的 `batch`
pub struct EmailAuthService {
    team48: Option<Arc<dyn Team48Onboard>>,
}

impl EmailAuthService {
    pub(crate) fn new(team48: Option<Arc<dyn Team48Onboard>>) -> Self {
        Self { team48 }
    }

    #[must_use]
    pub fn configured(&self) -> bool {
        self.team48.is_some()
    }

    fn port(&self) -> Result<&dyn Team48Onboard, AdminError> {
        self.team48
            .as_deref()
            .ok_or_else(|| AdminError::unavailable(TEAM48_NOT_CONFIGURED_MESSAGE))
    }

    pub async fn get(&self) -> Result<serde_json::Value, AdminError> {
        self.port()?.get().await.map_err(map_team48_error)
    }

    pub async fn submit(
        &self,
        emails: Vec<String>,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, AdminError> {
        self.port()?
            .submit(emails, settings)
            .await
            .map_err(map_team48_error)
    }

    pub async fn stop(&self) -> Result<serde_json::Value, AdminError> {
        self.port()?.stop().await.map_err(map_team48_error)
    }

    pub async fn retry(&self, email: String) -> Result<serde_json::Value, AdminError> {
        self.port()?.retry(email).await.map_err(map_team48_error)
    }
}

fn map_team48_error(error: Team48Error) -> AdminError {
    match error {
        Team48Error::Business {
            status, message, ..
        } => match status {
            Team48BusinessStatus::BadRequest => AdminError::bad_request(message),
            Team48BusinessStatus::NotFound => AdminError::not_found(message),
            Team48BusinessStatus::Conflict => AdminError::conflict(message),
            Team48BusinessStatus::ServiceUnavailable => AdminError::unavailable(message),
        },
        Team48Error::Unreachable => AdminError::bad_gateway("连不上 48team"),
        Team48Error::AuthFailed => AdminError::bad_gateway("48team 内部密钥不对或未配置"),
        Team48Error::InvalidResponse => AdminError::bad_gateway("48team 返回异常响应"),
    }
}
