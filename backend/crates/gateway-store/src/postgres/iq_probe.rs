//! 降智检测（fork 自有功能）的建表与 PostgreSQL 读写
//!
//! 三张表故意不进 `migrations/`，避免与原版迁移编号冲突
//! 启动时在原版迁移之后用 `if not exists` 幂等创建；回滚旧镜像时旧版直接忽略这些表

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gateway_admin::{
    model::iq_probe::{
        IqProbeAction, IqProbeKind, IqProbeOutcome, IqProbeRound, IqProbeRoundDetail,
        IqProbeScheduleRow, IqProbeSettings,
    },
    ports::{
        iq_probe::{IqProbeStore, StoredIqProbeSettings},
        store::{AdminStoreError, AdminStoreErrorKind, AdminStoreResult},
    },
};
use sqlx::{PgPool, Row as _, postgres::PgRow, types::Json};

use crate::{StoreResult, admin_store_error, postgres_unavailable};

const RESOURCE: &str = "iq probe";

const SCHEMA: &[&str] = &[
    "create table if not exists fork_iq_probe_settings (
        id smallint primary key check (id = 1),
        config jsonb not null,
        revision bigint not null default 1,
        slot_cursor_at timestamptz,
        updated_at timestamptz not null default now()
    )",
    "create table if not exists fork_iq_probe_schedule (
        account_id text primary key,
        next_due_at timestamptz not null,
        recheck_due_at timestamptz,
        last_started_at timestamptz
    )",
    "create table if not exists fork_iq_probe_rounds (
        id text primary key,
        account_id text not null,
        kind text not null check (kind in ('scheduled','recheck')),
        started_at timestamptz not null,
        finished_at timestamptz not null,
        outcome text not null check (outcome in ('passed','passed_after_retry','failed','error')),
        action text not null check (action in ('none','disabled','breaker_held')),
        detail jsonb not null
    )",
    "create index if not exists fork_iq_probe_rounds_account_idx on fork_iq_probe_rounds (account_id, started_at desc)",
    "create index if not exists fork_iq_probe_rounds_action_idx on fork_iq_probe_rounds (action, finished_at)",
];

/// 在原版迁移成功后创建 fork 自有表；只读连接不调用
pub(crate) async fn ensure_fork_iq_probe_schema(pool: &PgPool) -> StoreResult<()> {
    for &statement in SCHEMA {
        sqlx::query(statement)
            .execute(pool)
            .await
            .map_err(|source| postgres_unavailable("create fork iq probe tables", source))?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct PgIqProbeStore {
    pub pool: PgPool,
}

#[async_trait]
impl IqProbeStore for PgIqProbeStore {
    async fn load_settings(&self) -> AdminStoreResult<Option<StoredIqProbeSettings>> {
        let row = sqlx::query(
            "select config, revision, slot_cursor_at from fork_iq_probe_settings where id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|source| store_error("load iq probe settings", source))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let config: Json<IqProbeSettings> = row
            .try_get("config")
            .map_err(|source| store_error("decode iq probe settings", source))?;
        let revision: i64 = row
            .try_get("revision")
            .map_err(|source| store_error("decode iq probe revision", source))?;
        let cursor: Option<DateTime<Utc>> = row
            .try_get("slot_cursor_at")
            .map_err(|source| store_error("decode iq probe cursor", source))?;
        Ok(Some((
            config.0,
            u64::try_from(revision).map_err(|_| invalid("invalid revision"))?,
            cursor,
        )))
    }

    async fn save_settings(
        &self,
        settings: &IqProbeSettings,
        expected_revision: u64,
    ) -> AdminStoreResult<u64> {
        let expected =
            i64::try_from(expected_revision).map_err(|_| invalid("invalid revision"))?;
        // 期望版本为 0 时只允许首次插入；否则只更新版本一致的行
        let next: Option<i64> = if expected == 0 {
            sqlx::query_scalar(
                "insert into fork_iq_probe_settings (id, config, revision) values (1, $1, 1)
                 on conflict (id) do nothing returning revision",
            )
            .bind(Json(settings))
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query_scalar(
                "update fork_iq_probe_settings set config = $1, revision = revision + 1, updated_at = now()
                 where id = 1 and revision = $2 returning revision",
            )
            .bind(Json(settings))
            .bind(expected)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|source| store_error("save iq probe settings", source))?;
        let next = next.ok_or_else(|| {
            AdminStoreError::new(
                AdminStoreErrorKind::Conflict,
                RESOURCE,
                "settings revision changed",
            )
        })?;
        u64::try_from(next).map_err(|_| invalid("invalid revision"))
    }

    async fn set_slot_cursor(&self, at: Option<DateTime<Utc>>) -> AdminStoreResult<()> {
        // 没有设置行说明从未保存过，功能处于关闭状态，不会排队，无需写游标
        sqlx::query("update fork_iq_probe_settings set slot_cursor_at = $1 where id = 1")
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(|source| store_error("save iq probe cursor", source))?;
        Ok(())
    }

    async fn list_schedule(&self) -> AdminStoreResult<Vec<IqProbeScheduleRow>> {
        let rows = sqlx::query(
            "select account_id, next_due_at, recheck_due_at, last_started_at
             from fork_iq_probe_schedule order by account_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|source| store_error("list iq probe schedule", source))?;
        rows.iter().map(decode_schedule).collect()
    }

    async fn upsert_schedule(&self, row: &IqProbeScheduleRow) -> AdminStoreResult<()> {
        sqlx::query(
            "insert into fork_iq_probe_schedule (account_id, next_due_at, recheck_due_at, last_started_at)
             values ($1, $2, $3, $4)
             on conflict (account_id) do update set next_due_at = excluded.next_due_at,
                 recheck_due_at = excluded.recheck_due_at, last_started_at = excluded.last_started_at",
        )
        .bind(&row.account_id)
        .bind(row.next_due_at)
        .bind(row.recheck_due_at)
        .bind(row.last_started_at)
        .execute(&self.pool)
        .await
        .map_err(|source| store_error("save iq probe schedule", source))?;
        Ok(())
    }

    async fn clear_schedule(&self) -> AdminStoreResult<()> {
        sqlx::query("delete from fork_iq_probe_schedule")
            .execute(&self.pool)
            .await
            .map_err(|source| store_error("clear iq probe schedule", source))?;
        Ok(())
    }

    async fn delete_orphan_schedule(&self) -> AdminStoreResult<u64> {
        let result = sqlx::query(
            "delete from fork_iq_probe_schedule s
             where not exists (select 1 from provider_accounts a where a.id = s.account_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|source| store_error("delete orphan iq probe schedule", source))?;
        Ok(result.rows_affected())
    }

    async fn insert_round(&self, round: &IqProbeRound) -> AdminStoreResult<()> {
        sqlx::query(
            "insert into fork_iq_probe_rounds
                 (id, account_id, kind, started_at, finished_at, outcome, action, detail)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&round.id)
        .bind(&round.account_id)
        .bind(round.kind.as_str())
        .bind(round.started_at)
        .bind(round.finished_at)
        .bind(round.outcome.as_str())
        .bind(round.action.as_str())
        .bind(Json(&round.detail))
        .execute(&self.pool)
        .await
        .map_err(|source| store_error("insert iq probe round", source))?;
        Ok(())
    }

    async fn list_recent_rounds(
        &self,
        account_ids: &[String],
        per_account: u32,
    ) -> AdminStoreResult<Vec<IqProbeRound>> {
        if account_ids.is_empty() || per_account == 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "select id, account_id, kind, started_at, finished_at, outcome, action, detail
             from (
                 select r.*, row_number() over (partition by account_id order by started_at desc, id desc) as rank
                 from fork_iq_probe_rounds r
                 where account_id = any($1)
             ) ranked
             where rank <= $2
             order by account_id, started_at desc, id desc",
        )
        .bind(account_ids)
        .bind(i64::from(per_account))
        .fetch_all(&self.pool)
        .await
        .map_err(|source| store_error("list iq probe rounds", source))?;
        rows.iter().map(decode_round).collect()
    }

    async fn count_actions_since(
        &self,
        action: IqProbeAction,
        since: DateTime<Utc>,
    ) -> AdminStoreResult<u32> {
        let count: i64 = sqlx::query_scalar(
            "select count(*) from fork_iq_probe_rounds where action = $1 and finished_at >= $2",
        )
        .bind(action.as_str())
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(|source| store_error("count iq probe actions", source))?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    async fn delete_rounds_before(&self, before: DateTime<Utc>) -> AdminStoreResult<u64> {
        let result = sqlx::query("delete from fork_iq_probe_rounds where started_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await
            .map_err(|source| store_error("delete expired iq probe rounds", source))?;
        Ok(result.rows_affected())
    }
}

fn decode_schedule(row: &PgRow) -> AdminStoreResult<IqProbeScheduleRow> {
    let decode = |source| store_error("decode iq probe schedule", source);
    Ok(IqProbeScheduleRow {
        account_id: row.try_get("account_id").map_err(decode)?,
        next_due_at: row.try_get("next_due_at").map_err(decode)?,
        recheck_due_at: row.try_get("recheck_due_at").map_err(decode)?,
        last_started_at: row.try_get("last_started_at").map_err(decode)?,
    })
}

fn decode_round(row: &PgRow) -> AdminStoreResult<IqProbeRound> {
    let decode = |source| store_error("decode iq probe round", source);
    let kind: String = row.try_get("kind").map_err(decode)?;
    let outcome: String = row.try_get("outcome").map_err(decode)?;
    let action: String = row.try_get("action").map_err(decode)?;
    let detail: Json<IqProbeRoundDetail> = row.try_get("detail").map_err(decode)?;
    Ok(IqProbeRound {
        id: row.try_get("id").map_err(decode)?,
        account_id: row.try_get("account_id").map_err(decode)?,
        kind: IqProbeKind::parse(&kind).ok_or_else(|| invalid("unknown round kind"))?,
        started_at: row.try_get("started_at").map_err(decode)?,
        finished_at: row.try_get("finished_at").map_err(decode)?,
        outcome: IqProbeOutcome::parse(&outcome).ok_or_else(|| invalid("unknown round outcome"))?,
        action: IqProbeAction::parse(&action).ok_or_else(|| invalid("unknown round action"))?,
        detail: detail.0,
    })
}

fn invalid(message: &'static str) -> AdminStoreError {
    AdminStoreError::new(AdminStoreErrorKind::Invalid, RESOURCE, message)
}

fn store_error(operation: &'static str, source: sqlx::Error) -> AdminStoreError {
    admin_store_error(RESOURCE, postgres_unavailable(operation, source))
}
