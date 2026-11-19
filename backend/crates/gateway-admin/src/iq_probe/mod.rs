//! 降智检测（fork 自有功能）的设置读写、结果查询与熔断状态
//!
//! 定时考试与停号由 `worker` 执行，回答判定在 `judge`

pub(crate) mod judge;
pub(crate) mod worker;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};

use crate::model::iq_probe::{
    IQ_PROBE_BREAKER_WINDOW_MINUTES, IQ_PROBE_MAX_ROUND_ACCOUNTS,
    IQ_PROBE_RECENT_ROUNDS_PER_ACCOUNT, IqProbeAction, IqProbeRound, IqProbeSettings,
    IqProbeSettingsView, IqProbeStatus,
};
use crate::model::{AdminError, AdminErrorKind, MutationContext};
use crate::ports::iq_probe::IqProbeStore;
use crate::ports::store::{AdminStoreError, AdminStoreErrorKind};

/// 管理接口使用的降智检测能力
#[async_trait]
pub trait IqProbeService: Send + Sync {
    /// 没有设置行时返回默认设置，revision 为 0
    async fn settings(&self) -> Result<IqProbeSettingsView, AdminError>;

    /// 校验后按 revision 保存；从关闭变为开启时排队从头开始
    async fn update_settings(
        &self,
        context: &MutationContext,
        settings: IqProbeSettings,
        revision: u64,
    ) -> Result<IqProbeSettingsView, AdminError>;

    /// 每个账号最近 24 轮，新的在前；最多 200 个账号
    async fn recent_rounds(&self, account_ids: &[String]) -> Result<Vec<IqProbeRound>, AdminError>;

    async fn status(&self) -> Result<IqProbeStatus, AdminError>;
}

pub(crate) struct DefaultIqProbeService {
    store: Option<Arc<dyn IqProbeStore>>,
}

impl DefaultIqProbeService {
    /// 存储未挂载时所有操作返回不可用，只在不含 fork 表的测试组装中出现
    pub(crate) fn new(store: Option<Arc<dyn IqProbeStore>>) -> Self {
        Self { store }
    }

    fn store(&self) -> Result<&dyn IqProbeStore, AdminError> {
        self.store
            .as_deref()
            .ok_or_else(|| AdminError::unavailable("降智检测存储未启用"))
    }

    async fn load(&self) -> Result<(IqProbeSettings, u64), AdminError> {
        Ok(self
            .store()?
            .load_settings()
            .await
            .map_err(map_store_error)?
            .map_or_else(
                || (IqProbeSettings::default(), 0),
                |(settings, revision, _)| (settings, revision),
            ))
    }

    async fn status_for(&self, settings: &IqProbeSettings) -> Result<IqProbeStatus, AdminError> {
        let since = Utc::now() - Duration::minutes(i64::from(IQ_PROBE_BREAKER_WINDOW_MINUTES));
        let disabled = self
            .store()?
            .count_actions_since(IqProbeAction::Disabled, since)
            .await
            .map_err(map_store_error)?;
        Ok(IqProbeStatus {
            breaker_active: disabled >= settings.breaker_threshold,
            auto_disabled_in_window: disabled,
            window_minutes: IQ_PROBE_BREAKER_WINDOW_MINUTES,
        })
    }
}

#[async_trait]
impl IqProbeService for DefaultIqProbeService {
    async fn settings(&self) -> Result<IqProbeSettingsView, AdminError> {
        let (settings, revision) = self.load().await?;
        let status = self.status_for(&settings).await?;
        Ok(IqProbeSettingsView {
            settings,
            revision,
            status,
        })
    }

    async fn update_settings(
        &self,
        context: &MutationContext,
        settings: IqProbeSettings,
        revision: u64,
    ) -> Result<IqProbeSettingsView, AdminError> {
        let settings = settings.normalized();
        settings.validate().map_err(AdminError::invalid)?;
        let store = self.store()?;
        let (previous, _) = self.load().await?;
        let revision = store
            .save_settings(&settings, revision)
            .await
            .map_err(map_store_error)?;
        if settings.enabled && !previous.enabled {
            // 重新开启时旧排期已过时，清空后按账号顺序重新错开
            store.clear_schedule().await.map_err(map_store_error)?;
            store.set_slot_cursor(None).await.map_err(map_store_error)?;
        }
        tracing::info!(
            request_id = %context.request_id,
            enabled = settings.enabled,
            revision,
            "降智检测设置已保存"
        );
        let status = self.status_for(&settings).await?;
        Ok(IqProbeSettingsView {
            settings,
            revision,
            status,
        })
    }

    async fn recent_rounds(&self, account_ids: &[String]) -> Result<Vec<IqProbeRound>, AdminError> {
        if account_ids.len() > IQ_PROBE_MAX_ROUND_ACCOUNTS {
            return Err(AdminError::invalid(format!(
                "一次最多查询 {IQ_PROBE_MAX_ROUND_ACCOUNTS} 个账号"
            )));
        }
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.store()?
            .list_recent_rounds(account_ids, IQ_PROBE_RECENT_ROUNDS_PER_ACCOUNT)
            .await
            .map_err(map_store_error)
    }

    async fn status(&self) -> Result<IqProbeStatus, AdminError> {
        let (settings, _) = self.load().await?;
        self.status_for(&settings).await
    }
}

fn map_store_error(error: AdminStoreError) -> AdminError {
    let (kind, message) = match error.kind() {
        AdminStoreErrorKind::StaleRevision
        | AdminStoreErrorKind::DuplicateName
        | AdminStoreErrorKind::Conflict => {
            (AdminErrorKind::Conflict, "降智检测设置已被修改，请刷新后重试")
        }
        AdminStoreErrorKind::Invalid | AdminStoreErrorKind::NotFound => {
            (AdminErrorKind::Invalid, "降智检测数据不合法")
        }
        AdminStoreErrorKind::Unavailable => (AdminErrorKind::Unavailable, "降智检测存储暂不可用"),
    };
    AdminError::new(kind, message).with_source(error)
}
