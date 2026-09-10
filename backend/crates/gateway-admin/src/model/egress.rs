//! Proxy administration contracts; endpoint secrets are write-only in the HTTP API.

use super::Revision;
use gateway_core::egress::ProxyEndpoint;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct EgressProxyRecord {
    pub id: String,
    pub name: String,
    pub address: String,
    pub enabled: bool,
    pub revision: u64,
    pub account_count: u64,
}

#[derive(Debug, Clone)]
pub struct EgressDirectory {
    pub proxies: Vec<EgressProxyRecord>,
    pub bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum EgressMutation {
    Save {
        id: String,
        name: String,
        endpoint: Option<ProxyEndpoint>,
        enabled: bool,
        /// None creates a new node, Some fences an existing node update.
        expected_revision: Option<u64>,
    },
    Delete {
        id: String,
        expected_revision: u64,
    },
    Bind {
        account_ids: Vec<String>,
        proxy_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct EgressMutationResult {
    pub config_revision: Revision,
}
