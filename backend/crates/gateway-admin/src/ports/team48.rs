//! 48team 邮箱自动授权内部接口的端口契约、连接配置与错误分类

use std::fmt;

use async_trait::async_trait;
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const MINIMUM_TEAM48_API_KEY_CHARS: usize = 24;

/// 连接同机 48team 内部接口的启动配置；省略整段表示未启用该能力
#[derive(Clone, Deserialize)]
pub struct ForkTeam48Config {
    pub base_url: String,
    pub api_key: SecretString,
}

impl ForkTeam48Config {
    /// 校验地址协议与内部密钥长度，并规范化首尾空白与末尾斜杠
    ///
    /// # Errors
    ///
    /// 地址不以 `http://` 或 `https://` 开头，或密钥不足 24 位时返回错误字段名
    pub fn resolve_and_validate(&mut self) -> Result<(), &'static str> {
        let trimmed_url = self.base_url.trim().trim_end_matches('/');
        let valid_url = trimmed_url
            .strip_prefix("http://")
            .or_else(|| trimmed_url.strip_prefix("https://"))
            .is_some_and(|rest| {
                !rest.is_empty() && !rest.chars().any(|ch| ch.is_whitespace() || ch.is_control())
            });
        if !valid_url {
            return Err("fork_team48.base_url");
        }

        let trimmed_key = self.api_key.expose_secret().trim();
        if trimmed_key.chars().count() < MINIMUM_TEAM48_API_KEY_CHARS
            || trimmed_key.chars().any(char::is_control)
        {
            return Err("fork_team48.api_key");
        }

        self.base_url = trimmed_url.to_owned();
        if trimmed_key.len() != self.api_key.expose_secret().len() {
            self.api_key = SecretString::from(trimmed_key.to_owned());
        }
        Ok(())
    }
}

impl fmt::Debug for ForkTeam48Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ForkTeam48Config")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// 48team 内部接口返回的受控业务错误状态码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team48BusinessStatus {
    BadRequest,
    NotFound,
    Conflict,
    ServiceUnavailable,
}

/// 调用 48team 内部接口的错误分类；不包含原始密钥、邮箱或响应体原文
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Team48Error {
    /// 48team 返回的业务错误（400 / 404 / 409 / 503）及其脱敏中文说明
    #[error("{message}")]
    Business {
        status: Team48BusinessStatus,
        code: String,
        message: String,
    },
    /// 连接失败或请求超时
    #[error("连不上 48team")]
    Unreachable,
    /// 48team 返回 401，或因未配置内部密钥返回 404
    #[error("48team 内部密钥不对或未配置")]
    AuthFailed,
    /// 48team 返回非预期状态码或无法解析的响应结构
    #[error("48team 返回异常响应")]
    InvalidResponse,
}

/// 转发邮箱自动授权批次请求的外部端口；返回的 `Value` 为 48team 的 `batch` 字段原值
#[async_trait]
pub trait Team48Onboard: Send + Sync {
    async fn get(&self) -> Result<serde_json::Value, Team48Error>;

    async fn submit(
        &self,
        emails: Vec<String>,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, Team48Error>;

    async fn stop(&self) -> Result<serde_json::Value, Team48Error>;

    async fn retry(&self, email: String) -> Result<serde_json::Value, Team48Error>;
}
