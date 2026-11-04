//! 逻辑会话账号绑定与线程传输隔离键的单向派生

use gateway_core::operation::RawJsonPayload;
use gateway_core::policy::ClientApiKeyId;
use gateway_core::provider_ports::{ProviderSessionAffinityKey, ProviderSessionAlias};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::request_identity::{account_session_with_headers, follows_session_with_headers};
use crate::transport::protocol::responses::CodexResponsesRequest;
use crate::transport::request::derive_conversation_anchor;

const AFFINITY_KEY_HASH_LENGTH: usize = 12;

/// 一次请求派生出的账号亲和键及其结构化日志上下文
pub(crate) struct CodexSessionAffinity {
    key: ProviderSessionAffinityKey,
    turn_alias: Option<(ProviderSessionAffinityKey, ProviderSessionAlias)>,
    key_hash: String,
    anchor_source: &'static str,
    anchor: String,
    session_id: Option<String>,
    follow_only: bool,
    carries_history: bool,
    portable_history_tail: Option<String>,
}

impl CodexSessionAffinity {
    pub(crate) fn from_turn_alias(
        turn: ProviderSessionAffinityKey,
        alias: ProviderSessionAlias,
    ) -> Self {
        // 旧轮次可能登记在线程绑定下，统一还原为会话身份并保留原记录用于续期
        let root = alias.root_session_key.as_ref();
        let key = root.unwrap_or(&alias.session_key).clone();
        Self {
            key_hash: short_key_hash(&key),
            key,
            anchor_source: "turn-session",
            anchor: String::new(),
            session_id: None,
            follow_only: root.is_some() || alias.follow_only,
            carries_history: false,
            portable_history_tail: None,
            turn_alias: Some((turn, alias)),
        }
    }

    pub(crate) fn turn_alias(&self) -> Option<&ProviderSessionAffinityKey> {
        self.turn_alias.as_ref().map(|(key, _)| key)
    }

    pub(crate) fn alias_record(&self) -> ProviderSessionAlias {
        // 当前请求可加严跟随约束，续期仍保留轮次最初登记的身份，不能改写关联
        self.turn_alias
            .as_ref()
            .map(|(_, alias)| alias.clone())
            .unwrap_or_else(|| ProviderSessionAlias {
                session_key: self.key.clone(),
                follow_only: self.follow_only,
                root_session_key: None,
            })
    }

    pub(crate) const fn follow_only(&self) -> bool {
        self.follow_only
    }

    pub(crate) fn with_follow_only(mut self, follow_only: bool) -> Self {
        self.follow_only |= follow_only;
        self
    }

    /// 请求历史里带有主账号生成的不透明内容；strict 下只有这类请求不能落到会话主账号以外的账号
    pub(crate) const fn carries_history(&self) -> bool {
        self.carries_history
    }

    /// 历史里最后一段主账号加密内容的指纹；历史含压缩项或服务端条目引用时为 None，
    /// 这类内容剥离后会丢失记忆，不能靠剥离换号
    pub(crate) fn portable_history_tail(&self) -> Option<&str> {
        self.portable_history_tail.as_deref()
    }

    #[must_use]
    pub(crate) const fn key(&self) -> &ProviderSessionAffinityKey {
        &self.key
    }

    #[must_use]
    pub(crate) fn key_hash(&self) -> &str {
        &self.key_hash
    }

    /// 返回可持久化的客户端作用域不透明会话关联值
    #[must_use]
    pub(crate) fn persistence_hash(&self) -> &str {
        self.key.expose_to_store()
    }

    #[must_use]
    pub(crate) const fn anchor_source(&self) -> &'static str {
        self.anchor_source
    }

    #[must_use]
    pub(crate) fn anchor(&self) -> &str {
        &self.anchor
    }

    #[must_use]
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    #[must_use]
    pub(crate) const fn session_id_present(&self) -> bool {
        self.session_id.is_some()
    }

    #[must_use]
    pub(crate) fn into_key(self) -> ProviderSessionAffinityKey {
        self.key
    }
}

/// 将原始 response ID 投影为客户端作用域的不可逆关联值
#[must_use]
pub(crate) fn derive_previous_response_id_hash(
    previous_response_id: &str,
    client_api_key_id: &ClientApiKeyId,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"codex-previous-response-observation-v1\0");
    hasher.update(client_api_key_id.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(previous_response_id.as_bytes());
    hex::encode(hasher.finalize())
}

pub(crate) fn derive_codex_session_affinity(
    request: &CodexResponsesRequest,
    client_api_key_id: &ClientApiKeyId,
) -> Option<CodexSessionAffinity> {
    let session_id = non_empty(request.client_account_session_id.as_deref())?;
    session_affinity(
        "root-session",
        session_id.to_owned(),
        Some(session_id.to_owned()),
        client_api_key_id,
    )
    .map(|affinity| {
        let mut affinity = affinity.with_follow_only(request.client_account_follow_only);
        affinity.carries_history = carries_conversation_history(request.body());
        affinity.portable_history_tail = affinity
            .carries_history
            .then(|| portable_history_tail(request.body()))
            .flatten();
        affinity
    })
}

fn encrypted_content(value: &Value) -> Option<&str> {
    value
        .get("encrypted_content")
        .and_then(Value::as_str)
        .filter(|content| !content.is_empty())
}

fn content_fingerprint(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"codex-bound-history-v1\0");
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// 条目自身及其消息片段里的全部加密内容，按出现顺序
fn item_encrypted_contents(item: &Value) -> impl Iterator<Item = &str> {
    encrypted_content(item).into_iter().chain(
        item.get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(encrypted_content),
    )
}

fn portable_history_tail(body: &Map<String, Value>) -> Option<String> {
    let items = body.get("input")?.as_array()?;
    if items.iter().any(|item| {
        matches!(
            item.get("type").and_then(Value::as_str),
            Some("compaction" | "compaction_summary" | "item_reference")
        )
    }) {
        return None;
    }
    items
        .iter()
        .flat_map(item_encrypted_contents)
        .last()
        .map(content_fingerprint)
}

/// 删除迁移分界点（含）之前旧账号生成的加密内容：加密推理条目整条删除，
/// 消息里的加密片段单独删除，分界内条目的上游 id 一并去掉；明文消息和工具调用保留。
/// 找不到分界点（如之后已在新账号压缩）时不改动，返回删除的加密内容数
pub(crate) fn strip_bound_history_through(body: &mut Map<String, Value>, through: &str) -> usize {
    let Some(Value::Array(items)) = body.get_mut("input") else {
        return 0;
    };
    let Some(boundary) = items.iter().rposition(|item| {
        item_encrypted_contents(item).any(|content| content_fingerprint(content) == through)
    }) else {
        return 0;
    };
    let mut stripped = 0;
    let mut kept = Vec::with_capacity(items.len());
    for (index, mut item) in std::mem::take(items).into_iter().enumerate() {
        if index > boundary {
            kept.push(item);
            continue;
        }
        if encrypted_content(&item).is_some() {
            stripped += 1;
            continue;
        }
        if let Some(object) = item.as_object_mut() {
            object.remove("id");
            if let Some(Value::Array(parts)) = object.get_mut("content") {
                let before = parts.len();
                parts.retain(|part| encrypted_content(part).is_none());
                if parts.len() != before {
                    stripped += before - parts.len();
                    if parts.is_empty() {
                        continue;
                    }
                }
            }
        }
        kept.push(item);
    }
    *items = kept;
    stripped
}

/// 已剥离迁移会话的分界点存放键：按会话和迁入账号区分，旧账号换回时不会读到其他账号的分界点
pub(crate) fn bound_history_marker_key(
    session: &ProviderSessionAffinityKey,
    account_id: &str,
) -> Option<ProviderSessionAffinityKey> {
    opaque_affinity_key(
        "bound-history-marker",
        &format!("{}\0{account_id}", session.expose_to_store()),
    )
}

/// 分界点指纹本身就是小写十六进制，可直接存成别名目标键
pub(crate) fn bound_history_marker_alias(tail: &str) -> Option<ProviderSessionAlias> {
    Some(ProviderSessionAlias {
        session_key: ProviderSessionAffinityKey::try_new(tail).ok()?,
        root_session_key: None,
        follow_only: false,
    })
}

/// 只有上游账号生成的不透明内容会把历史绑在主账号上：加密推理、压缩项、服务端条目引用，
/// 以及消息里的加密片段。助手文本、工具调用及其结果换号重放不会关联账号，
/// Chat Completions 等转换来的历史只有这类明文条目，不算
fn carries_conversation_history(body: &Map<String, Value>) -> bool {
    let has_encrypted = |value: &Value| {
        value
            .get("encrypted_content")
            .and_then(Value::as_str)
            .is_some_and(|content| !content.is_empty())
    };
    body.get("input")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| match item.get("type").and_then(Value::as_str) {
                    Some("compaction" | "compaction_summary" | "item_reference") => true,
                    _ => {
                        has_encrypted(item)
                            || item
                                .get("content")
                                .and_then(Value::as_array)
                                .is_some_and(|parts| parts.iter().any(has_encrypted))
                    }
                })
        })
}

/// 原生 JSON 端点与 Responses 共用绑定命名空间，Search id 是逻辑会话身份
pub(crate) fn derive_codex_endpoint_session_affinity(
    payload: &RawJsonPayload,
    client_api_key_id: &ClientApiKeyId,
    body_session_field: &str,
) -> Option<CodexSessionAffinity> {
    derive_endpoint_affinity_with_headers(payload, client_api_key_id, body_session_field, &[])
}

pub(crate) fn derive_endpoint_affinity_with_headers(
    payload: &RawJsonPayload,
    client_api_key_id: &ClientApiKeyId,
    body_session_field: &str,
    headers: &[gateway_core::engine::middleware::MiddlewareHeader],
) -> Option<CodexSessionAffinity> {
    let mut body = serde_json::from_slice::<Map<String, Value>>(payload.body()).unwrap_or_default();
    if let Some(session) = body.get(body_session_field).cloned() {
        body.entry("session_id").or_insert(session);
    }
    let session_id = account_session_with_headers(&body, payload.context(), headers)?;
    session_affinity(
        "root-session",
        session_id.clone(),
        Some(session_id),
        client_api_key_id,
    )
    .map(|affinity| {
        affinity.with_follow_only(follows_session_with_headers(
            &body,
            payload.context(),
            headers,
        ))
    })
}

pub(crate) fn derive_live_session_affinity(
    request: &gateway_core::operation::ProviderHttpRequest,
    headers: &[gateway_core::engine::middleware::MiddlewareHeader],
    client_api_key_id: &ClientApiKeyId,
) -> Option<CodexSessionAffinity> {
    let mut effective = headers.to_vec();
    effective.extend(
        request
            .headers()
            .iter()
            .filter(|h| {
                !headers
                    .iter()
                    .any(|override_header| override_header.name().eq_ignore_ascii_case(h.name()))
            })
            .map(|h| {
                gateway_core::engine::middleware::MiddlewareHeader::new(h.name(), h.value().clone())
            }),
    );
    // x-session-id 是实时通话身份，不代表根对话；只消费官方发送的 session-id
    let session_id = account_session_with_headers(&Map::new(), &Map::new(), &effective)?;
    session_affinity(
        "root-session",
        session_id.clone(),
        Some(session_id),
        client_api_key_id,
    )
    .map(|affinity| {
        affinity.with_follow_only(follows_session_with_headers(
            &Map::new(),
            &Map::new(),
            &effective,
        ))
    })
}

/// HTTP 降级等传输状态仍按线程隔离，不能跟随整个对话的账号绑定扩大范围
pub(crate) fn derive_codex_transport_key(
    request: &CodexResponsesRequest,
    client_api_key_id: &ClientApiKeyId,
) -> Option<ProviderSessionAffinityKey> {
    let (source, anchor) = non_empty(request.client_thread_id.as_deref())
        .map(|thread| ("thread-transport", thread.to_owned()))
        .or_else(|| derive_conversation_anchor(request))?;
    session_affinity(source, anchor, None, client_api_key_id).map(CodexSessionAffinity::into_key)
}

fn session_affinity(
    anchor_source: &'static str,
    anchor: String,
    session_id: Option<String>,
    client_api_key_id: &ClientApiKeyId,
) -> Option<CodexSessionAffinity> {
    let session_key = opaque_affinity_key(anchor_source, &anchor)?;
    let key = opaque_affinity_key(
        "client-session",
        &format!(
            "{}\0{}",
            client_api_key_id.as_str(),
            session_key.expose_to_store()
        ),
    )?;
    let key_hash = short_key_hash(&key);
    Some(CodexSessionAffinity {
        key,
        turn_alias: None,
        key_hash,
        anchor_source,
        anchor,
        session_id,
        follow_only: false,
        carries_history: false,
        portable_history_tail: None,
    })
}

fn short_key_hash(key: &ProviderSessionAffinityKey) -> String {
    // 亲和键本身已经是 SHA-256；日志沿用 WebSocket 诊断的 12 位短哈希长度
    key.expose_to_store()
        .chars()
        .take(AFFINITY_KEY_HASH_LENGTH)
        .collect()
}

fn opaque_affinity_key(domain: &str, value: &str) -> Option<ProviderSessionAffinityKey> {
    let mut hasher = Sha256::new();
    hasher.update(b"codex-session-affinity-v1\0");
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(value.as_bytes());
    ProviderSessionAffinityKey::try_new(hex::encode(hasher.finalize())).ok()
}

/// 恢复旧版 `cyber_policy` 的会话隔离键
///
/// 它只接受显式 session/conversation 或客户端明确给出的 prompt cache key，避免将
/// 请求内容哈希误当成长会话；`previous_response_id` 续写不参与该策略
pub(crate) fn derive_codex_cyber_policy_session_key(
    request: &CodexResponsesRequest,
    client_api_key_id: &ClientApiKeyId,
) -> Option<ProviderSessionAffinityKey> {
    if request.previous_response_id().is_some() {
        return None;
    }
    let session_id = non_empty(request.client_session_id.as_deref())
        .or_else(|| non_empty(request.client_conversation_id.as_deref()))
        .or_else(|| {
            request
                .explicit_prompt_cache_key
                .then(|| request.prompt_cache_key())
                .flatten()
                .and_then(|value| non_empty(Some(value)))
        })?;
    let mut hasher = Sha256::new();
    hasher.update(b"cyber-policy-session\0");
    hasher.update(client_api_key_id.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(session_id.as_bytes());
    ProviderSessionAffinityKey::try_new(hex::encode(hasher.finalize())).ok()
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// 官方 Images 的轮次 ID 与 Responses turn_id 对应，关联必须隔离客户端 API Key
pub(crate) fn derive_turn_alias(
    turn_id: &str,
    client: &ClientApiKeyId,
) -> Option<ProviderSessionAffinityKey> {
    let turn_id = non_empty(Some(turn_id))?;
    opaque_affinity_key("client-turn", &format!("{}\0{turn_id}", client.as_str()))
}
