//! 降智检测的持久化端口：设置、账号排期与轮次结果都在 fork 自有表中

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::store::AdminStoreResult;
use crate::model::iq_probe::{IqProbeAction, IqProbeRound, IqProbeScheduleRow, IqProbeSettings};

/// 已保存的设置、版本与错开排队游标
pub type StoredIqProbeSettings = (IqProbeSettings, u64, Option<DateTime<Utc>>);

#[async_trait]
pub trait IqProbeStore: Send + Sync {
    /// 没有设置行时返回 `None`，由调用方使用默认设置
    async fn load_settings(&self) -> AdminStoreResult<Option<StoredIqProbeSettings>>;

    /// 按 revision 比较写入并返回新版本；从无到有时 `expected_revision` 为 0
    ///
    /// revision 不符时返回 `Conflict`
    async fn save_settings(
        &self,
        settings: &IqProbeSettings,
        expected_revision: u64,
    ) -> AdminStoreResult<u64>;

    /// 记录最近一次分配出去的首次开测时间；`None` 让排队从头开始
    async fn set_slot_cursor(&self, at: Option<DateTime<Utc>>) -> AdminStoreResult<()>;

    async fn list_schedule(&self) -> AdminStoreResult<Vec<IqProbeScheduleRow>>;

    /// 按账号整行覆盖排期
    async fn upsert_schedule(&self, row: &IqProbeScheduleRow) -> AdminStoreResult<()>;

    async fn clear_schedule(&self) -> AdminStoreResult<()>;

    /// 删除账号已不存在的排期，返回删除行数
    async fn delete_orphan_schedule(&self) -> AdminStoreResult<u64>;

    /// 写入一轮结果；`round.id` 由调用方用 [`IqProbeRound::new_id`] 生成
    async fn insert_round(&self, round: &IqProbeRound) -> AdminStoreResult<()>;

    /// 每个账号按开测时间倒序取最近 `per_account` 轮
    async fn list_recent_rounds(
        &self,
        account_ids: &[String],
        per_account: u32,
    ) -> AdminStoreResult<Vec<IqProbeRound>>;

    /// 统计 `finished_at >= since` 且处置为 `action` 的轮次数
    async fn count_actions_since(
        &self,
        action: IqProbeAction,
        since: DateTime<Utc>,
    ) -> AdminStoreResult<u32>;

    /// 删除开测时间早于 `before` 的轮次，返回删除行数
    async fn delete_rounds_before(&self, before: DateTime<Utc>) -> AdminStoreResult<u64>;
}
