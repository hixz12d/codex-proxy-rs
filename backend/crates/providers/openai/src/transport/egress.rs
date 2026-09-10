//! Explicit per-account clients; process proxy environment is never used.

use super::tls::{
    build_reqwest_client_with_custom_ca, custom_ca_env_cache_key, ensure_rustls_provider,
};
use super::{CodexBackendClient, CodexClientError};
use gateway_core::egress::AccountProxyRoute;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

type Cache = HashMap<(String, Option<String>), (Client, Instant)>;

pub(crate) fn proxy_http_client(route: &AccountProxyRoute) -> Result<Client, CodexClientError> {
    ensure_rustls_provider();
    static CLIENTS: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (
        format!(
            "{}:{}",
            route.key,
            hex::encode(Sha256::digest(route.endpoint.expose()))
        ),
        custom_ca_env_cache_key(),
    );
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((client, used)) = cache.get_mut(&key) {
        *used = Instant::now();
        return Ok(client.clone());
    }
    let proxy = reqwest::Proxy::all(route.endpoint.expose())
        .map_err(|_| CodexClientError::EgressUnavailable)?;
    let builder = Client::builder()
        .use_rustls_tls()
        .no_proxy()
        .proxy(proxy)
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15))
        .tcp_keepalive(Duration::from_secs(30))
        .http2_keep_alive_interval(Duration::from_secs(30))
        .http2_keep_alive_timeout(Duration::from_secs(5))
        .http2_keep_alive_while_idle(true);
    let client = build_reqwest_client_with_custom_ca(builder)
        .map_err(|_| CodexClientError::EgressUnavailable)?;
    if cache.len() >= 256
        && let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, (_, used))| *used)
            .map(|(key, _)| key.clone())
    {
        cache.remove(&oldest);
    }
    cache.insert(key, (client.clone(), Instant::now()));
    Ok(client)
}

impl CodexBackendClient {
    pub fn with_proxy(&self, route: Option<&AccountProxyRoute>) -> Result<Self, CodexClientError> {
        let mut scoped = self.clone();
        if let Some(route) = route {
            scoped.client = proxy_http_client(route)?;
            scoped.proxy = Some(route.endpoint.clone());
            scoped.websocket_origin_key = format!("{}:{}", self.websocket_origin_key, route.key);
            scoped.egress_key = route.key.clone();
        }
        Ok(scoped)
    }
}
