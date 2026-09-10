use super::{TestDatabase, admin_account_store};
use gateway_admin::{
    model::{MutationActor, MutationContext, egress::EgressMutation},
    ports::store::{AccountStore, AdminStoreErrorKind},
};
use gateway_core::{
    account::{ProviderAccountId, ProviderAccountStore},
    egress::ProxyEndpoint,
};
use gateway_store::postgres::PgProviderAccountRepository;

fn context() -> MutationContext {
    MutationContext {
        actor: MutationActor::System,
        request_id: "test-egress".to_owned(),
    }
}
fn save(enabled: bool, revision: Option<u64>) -> EgressMutation {
    EgressMutation::Save {
        id: "proxy_test".to_owned(),
        name: "Test exit".to_owned(),
        endpoint: Some(ProxyEndpoint::parse("socks5h://user:secret-test@127.0.0.1:1080").unwrap()),
        enabled,
        expected_revision: revision,
    }
}
async fn seed(pool: &sqlx::PgPool, id: &str, provider: &str) {
    sqlx::query("INSERT INTO provider_accounts(id,provider_kind,name,upstream_user_id,authentication_kind,provider_credentials_json,credential_revision,has_refresh_token,enabled,credential_state,credential_observed_at,created_at,updated_at)
        VALUES($1,$2,'Test account',$1,'oauth','{}'::jsonb,1,false,true,'ready',now(),now(),now())")
        .bind(id).bind(provider).execute(pool).await.unwrap();
}

#[tokio::test]
async fn egress_binding_is_fresh_fail_closed_and_cannot_delete_a_bound_node() {
    let Some(database) = TestDatabase::create("egress_lifecycle").await else {
        return;
    };
    seed(&database.pool, "acct_egress_one", "openai").await;
    seed(&database.pool, "acct_egress_two", "openai").await;
    let store = admin_account_store(&database.pool);
    let runtime = PgProviderAccountRepository::new(database.pool.clone());
    let account = ProviderAccountId::new("acct_egress_one").unwrap();
    assert!(runtime.account_proxy(&account).await.unwrap().is_none());
    store
        .mutate_egress(save(true, None), &context())
        .await
        .unwrap();
    store
        .mutate_egress(
            EgressMutation::Bind {
                account_ids: vec![account.to_string()],
                proxy_id: Some("proxy_test".to_owned()),
            },
            &context(),
        )
        .await
        .unwrap();
    let first = runtime.account_proxy(&account).await.unwrap().unwrap();
    assert!(
        runtime
            .account_proxy(&ProviderAccountId::new("acct_egress_two").unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let directory = store.egress_directory().await.unwrap();
    assert_eq!(directory.proxies[0].account_count, 1);
    assert_eq!(directory.proxies[0].address, "socks5h://127.0.0.1:1080");
    assert!(!format!("{directory:?}").contains("secret-test"));
    assert_eq!(
        store
            .mutate_egress(
                EgressMutation::Delete {
                    id: "proxy_test".to_owned(),
                    expected_revision: 1
                },
                &context()
            )
            .await
            .unwrap_err()
            .kind(),
        AdminStoreErrorKind::Conflict
    );
    store
        .mutate_egress(save(false, Some(1)), &context())
        .await
        .unwrap();
    assert!(runtime.account_proxy(&account).await.is_err());
    store
        .mutate_egress(save(true, Some(2)), &context())
        .await
        .unwrap();
    assert_ne!(
        first.key,
        runtime.account_proxy(&account).await.unwrap().unwrap().key
    );
    store
        .mutate_egress(
            EgressMutation::Bind {
                account_ids: vec![account.to_string()],
                proxy_id: None,
            },
            &context(),
        )
        .await
        .unwrap();
    assert!(runtime.account_proxy(&account).await.unwrap().is_none());
    store
        .mutate_egress(
            EgressMutation::Delete {
                id: "proxy_test".to_owned(),
                expected_revision: 3,
            },
            &context(),
        )
        .await
        .unwrap();
    assert!(store.egress_directory().await.unwrap().proxies.is_empty());
    database.close().await;
}

#[tokio::test]
async fn egress_cas_and_invalid_batch_roll_back_without_partial_bindings() {
    let Some(database) = TestDatabase::create("egress_cas").await else {
        return;
    };
    seed(&database.pool, "acct_egress_one", "openai").await;
    seed(&database.pool, "acct_egress_xai", "xai").await;
    let store = admin_account_store(&database.pool);
    store
        .mutate_egress(save(true, None), &context())
        .await
        .unwrap();
    store
        .mutate_egress(
            EgressMutation::Save {
                id: "proxy_test".to_owned(),
                name: "Renamed".to_owned(),
                endpoint: None,
                enabled: true,
                expected_revision: Some(1),
            },
            &context(),
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .mutate_egress(save(false, Some(1)), &context())
            .await
            .unwrap_err()
            .kind(),
        AdminStoreErrorKind::StaleRevision
    );
    let before: i64 = sqlx::query_scalar("SELECT config_revision FROM runtime_settings WHERE id=1")
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .mutate_egress(
                EgressMutation::Bind {
                    account_ids: vec!["acct_egress_one".to_owned(), "acct_egress_xai".to_owned()],
                    proxy_id: Some("proxy_test".to_owned())
                },
                &context()
            )
            .await
            .unwrap_err()
            .kind(),
        AdminStoreErrorKind::Invalid
    );
    let after: i64 = sqlx::query_scalar("SELECT config_revision FROM runtime_settings WHERE id=1")
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let directory = store.egress_directory().await.unwrap();
    assert!(directory.bindings.is_empty());
    assert_eq!(directory.proxies[0].revision, 2);
    let secret: String =
        sqlx::query_scalar("SELECT endpoint FROM egress_proxies WHERE id='proxy_test'")
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert!(secret.contains("secret-test"));
    let audits: String =
        sqlx::query_scalar("SELECT COALESCE(json_agg(a)::text,'[]') FROM admin_audit_events a")
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert!(!audits.contains("secret-test"));
    database.close().await;
}
