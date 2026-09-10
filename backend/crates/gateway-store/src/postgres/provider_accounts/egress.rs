//! Transactional proxy nodes and account bindings, separate from credential revisions.

use crate::postgres::{
    append_admin_audit_event_in_transaction, bump_config_revision_in_transaction,
};
use crate::{admin_revision, mutation_audit};
use gateway_admin::{
    model::{
        MutationContext,
        egress::{EgressDirectory, EgressMutation, EgressMutationResult, EgressProxyRecord},
    },
    ports::store::{AdminStoreError, AdminStoreErrorKind, AdminStoreResult},
};
use gateway_core::{
    account::ProviderAccountId,
    egress::{AccountProxyRoute, ProxyEndpoint},
    error::{StoreError, StoreErrorKind},
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn error(kind: AdminStoreErrorKind) -> AdminStoreError {
    AdminStoreError::new(kind, "egress", "proxy operation failed")
}
fn unavailable(_: impl std::fmt::Display) -> AdminStoreError {
    error(AdminStoreErrorKind::Unavailable)
}

pub(super) async fn resolve(
    pool: &PgPool,
    account: &ProviderAccountId,
) -> Result<Option<AccountProxyRoute>, StoreError> {
    let fail = || StoreError::new(StoreErrorKind::Unavailable);
    let row = sqlx::query(
        "SELECT b.proxy_id, b.generation, p.endpoint, p.enabled, p.revision
        FROM provider_accounts a LEFT JOIN account_egress_bindings b ON b.account_id=a.id
        LEFT JOIN egress_proxies p ON p.id=b.proxy_id WHERE a.id=$1",
    )
    .bind(account.as_str())
    .fetch_optional(pool)
    .await
    .map_err(|_| fail())?
    .ok_or_else(fail)?;
    let id: Option<String> = row.try_get("proxy_id").map_err(|_| fail())?;
    let Some(id) = id else { return Ok(None) };
    let enabled: bool = row.try_get("enabled").map_err(|_| fail())?;
    if !enabled {
        return Err(fail());
    }
    let endpoint: String = row.try_get("endpoint").map_err(|_| fail())?;
    let revision: i64 = row.try_get("revision").map_err(|_| fail())?;
    let generation: String = row.try_get("generation").map_err(|_| fail())?;
    Ok(Some(AccountProxyRoute {
        key: format!("{}:{id}:{revision}:{generation}", account.as_str()),
        endpoint: ProxyEndpoint::parse(&endpoint).map_err(|_| fail())?,
    }))
}

pub(super) async fn directory(pool: &PgPool) -> AdminStoreResult<EgressDirectory> {
    let rows = sqlx::query(
        "SELECT p.id,p.name,p.endpoint,p.enabled,p.revision,count(b.account_id) AS account_count
        FROM egress_proxies p LEFT JOIN account_egress_bindings b ON b.proxy_id=p.id
        GROUP BY p.id ORDER BY p.created_at,p.id",
    )
    .fetch_all(pool)
    .await
    .map_err(unavailable)?;
    let proxies = rows
        .into_iter()
        .map(|r| {
            let endpoint: String = r.try_get("endpoint").map_err(unavailable)?;
            Ok(EgressProxyRecord {
                id: r.try_get("id").map_err(unavailable)?,
                name: r.try_get("name").map_err(unavailable)?,
                address: ProxyEndpoint::parse(&endpoint)
                    .map_err(unavailable)?
                    .address(),
                enabled: r.try_get("enabled").map_err(unavailable)?,
                revision: u64::try_from(r.try_get::<i64, _>("revision").map_err(unavailable)?)
                    .map_err(unavailable)?,
                account_count: u64::try_from(
                    r.try_get::<i64, _>("account_count").map_err(unavailable)?,
                )
                .map_err(unavailable)?,
            })
        })
        .collect::<AdminStoreResult<Vec<_>>>()?;
    let rows = sqlx::query("SELECT account_id,proxy_id FROM account_egress_bindings")
        .fetch_all(pool)
        .await
        .map_err(unavailable)?;
    let bindings = rows
        .into_iter()
        .map(|r| {
            Ok((
                r.try_get("account_id").map_err(unavailable)?,
                r.try_get("proxy_id").map_err(unavailable)?,
            ))
        })
        .collect::<AdminStoreResult<_>>()?;
    Ok(EgressDirectory { proxies, bindings })
}

pub(super) async fn mutate(
    pool: &PgPool,
    command: EgressMutation,
    context: &MutationContext,
) -> AdminStoreResult<EgressMutationResult> {
    let mut tx = pool.begin().await.map_err(unavailable)?;
    let revision = bump_config_revision_in_transaction(&mut tx)
        .await
        .map_err(unavailable)?;
    let (action, target) = match command {
        EgressMutation::Save {
            id,
            name,
            endpoint,
            enabled,
            expected_revision,
        } => {
            if id.is_empty()
                || id.len() > 100
                || name.trim().is_empty()
                || name.len() > 100
                || name.chars().any(char::is_control)
            {
                return Err(error(AdminStoreErrorKind::Invalid));
            }
            if let Some(expected) = expected_revision {
                let result = sqlx::query("UPDATE egress_proxies SET name=$2,endpoint=COALESCE($3,endpoint),enabled=$4,revision=revision+1,updated_at=now() WHERE id=$1 AND revision=$5")
                    .bind(&id).bind(name.trim()).bind(endpoint.as_ref().map(ProxyEndpoint::expose)).bind(enabled)
                    .bind(i64::try_from(expected).map_err(|_| error(AdminStoreErrorKind::Invalid))?)
                    .execute(&mut *tx).await.map_err(unavailable)?;
                if result.rows_affected() != 1 {
                    return Err(error(AdminStoreErrorKind::StaleRevision));
                }
            } else {
                let endpoint = endpoint.ok_or_else(|| error(AdminStoreErrorKind::Invalid))?;
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM egress_proxies")
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(unavailable)?;
                if count >= 500 {
                    return Err(error(AdminStoreErrorKind::Invalid));
                }
                sqlx::query(
                    "INSERT INTO egress_proxies(id,name,endpoint,enabled) VALUES($1,$2,$3,$4)",
                )
                .bind(&id)
                .bind(name.trim())
                .bind(endpoint.expose())
                .bind(enabled)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
            }
            ("save", id)
        }
        EgressMutation::Delete {
            id,
            expected_revision,
        } => {
            let bound: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM account_egress_bindings WHERE proxy_id=$1)",
            )
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .map_err(unavailable)?;
            if bound {
                return Err(error(AdminStoreErrorKind::Conflict));
            }
            let result = sqlx::query("DELETE FROM egress_proxies WHERE id=$1 AND revision=$2")
                .bind(&id)
                .bind(
                    i64::try_from(expected_revision)
                        .map_err(|_| error(AdminStoreErrorKind::Invalid))?,
                )
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
            if result.rows_affected() != 1 {
                return Err(error(AdminStoreErrorKind::StaleRevision));
            }
            ("delete", id)
        }
        EgressMutation::Bind {
            mut account_ids,
            proxy_id,
        } => {
            account_ids.sort();
            account_ids.dedup();
            if account_ids.is_empty() || account_ids.len() > 1000 {
                return Err(error(AdminStoreErrorKind::Invalid));
            }
            let rows = sqlx::query("SELECT id,provider_kind FROM provider_accounts WHERE id=ANY($1) ORDER BY id FOR UPDATE")
                .bind(&account_ids).fetch_all(&mut *tx).await.map_err(unavailable)?;
            if rows.len() != account_ids.len()
                || rows
                    .iter()
                    .any(|r| r.get::<String, _>("provider_kind") != "openai")
            {
                return Err(error(AdminStoreErrorKind::Invalid));
            }
            if let Some(id) = &proxy_id {
                let valid: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM egress_proxies WHERE id=$1 AND enabled)",
                )
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(unavailable)?;
                if !valid {
                    return Err(error(AdminStoreErrorKind::Invalid));
                }
                for account in &account_ids {
                    sqlx::query("INSERT INTO account_egress_bindings(account_id,proxy_id,generation) VALUES($1,$2,$3)
                        ON CONFLICT(account_id) DO UPDATE SET proxy_id=excluded.proxy_id,generation=excluded.generation")
                        .bind(account).bind(id).bind(Uuid::now_v7().to_string()).execute(&mut *tx).await.map_err(unavailable)?;
                }
            } else {
                sqlx::query("DELETE FROM account_egress_bindings WHERE account_id=ANY($1)")
                    .bind(&account_ids)
                    .execute(&mut *tx)
                    .await
                    .map_err(unavailable)?;
            }
            for account in &account_ids {
                append_admin_audit_event_in_transaction(
                    &mut tx,
                    mutation_audit(context, "bind", "account_egress", account, Vec::new()),
                    revision,
                )
                .await
                .map_err(unavailable)?;
            }
            ("bind", "accounts".to_owned())
        }
    };
    append_admin_audit_event_in_transaction(
        &mut tx,
        mutation_audit(context, action, "egress_proxy", &target, Vec::new()),
        revision,
    )
    .await
    .map_err(unavailable)?;
    tx.commit().await.map_err(unavailable)?;
    Ok(EgressMutationResult {
        config_revision: admin_revision(revision)?,
    })
}
