//! 每个账号独立的 Codex Desktop 指纹。
//!
//! 只由账号永久不变的 `installation_id` 确定性推导，与账号顺序、数量无关；
//! 不入库，开关关闭或解析失败时调用方保持原有身份。

use std::sync::atomic::Ordering;

use sha2::{Digest, Sha256};

use super::selection::{ClientKind, ClientPlatform, ClientProfileSelection, VersionMode};
use super::{CodexWireProfile, CodexWireProfileState};

/// 域分隔前缀，避免与其他基于 installation_id 的派生值（如 surface stable id）相关。
const ACCOUNT_FINGERPRINT_DOMAIN: &[u8] = b"codex-proxy-rs/account-fingerprint/v1\0";

// 只使用内置官方版本基线覆盖的组合：macOS arm64、Windows x86_64、Linux x86_64。
// 值必须能通过 `ClientProfileSelection::validate`，不能含 `(`、`)`、`;`、`\`。
const MACOS_VERSIONS: [&str; 5] = ["14.7.8", "15.6.1", "15.7.1", "15.7.3", "26.0.1"];
const WINDOWS_VERSIONS: [&str; 4] = ["10.0.19045", "10.0.22631", "10.0.26100", "10.0.26200"];
const LINUX_VERSIONS: [&str; 3] = ["6.8.0", "6.11.0", "6.14.0"];

/// 按 installation_id 生成账号的 Desktop 选择：平台按 macOS 60 / Windows 30 / Linux 10 分布，
/// 架构走平台默认值，版本跟随官方最新。
pub(crate) fn account_selection(installation_id: &str) -> ClientProfileSelection {
    let mut digest = Sha256::new();
    digest.update(ACCOUNT_FINGERPRINT_DOMAIN);
    digest.update(installation_id.as_bytes());
    let digest = digest.finalize();
    let mut platform_bytes = [0_u8; 8];
    platform_bytes.copy_from_slice(&digest[..8]);
    let mut version_bytes = [0_u8; 8];
    version_bytes.copy_from_slice(&digest[8..16]);
    let platform_hash = u64::from_be_bytes(platform_bytes);
    let version_hash = u64::from_be_bytes(version_bytes);
    let (platform, versions): (ClientPlatform, &[&str]) = match platform_hash % 100 {
        0..60 => (ClientPlatform::Macos, &MACOS_VERSIONS),
        60..90 => (ClientPlatform::Windows, &WINDOWS_VERSIONS),
        _ => (ClientPlatform::Linux, &LINUX_VERSIONS),
    };
    let index = usize::try_from(version_hash % versions.len() as u64).unwrap_or_default();
    ClientProfileSelection {
        client: ClientKind::Desktop,
        platform,
        version_mode: VersionMode::Latest,
        os_version: versions.get(index).map(|version| (*version).to_owned()),
        ..ClientProfileSelection::default()
    }
}

impl CodexWireProfileState {
    /// 更新“每个账号独立指纹”运行时开关；所有 clone 共享同一个值。
    pub fn set_account_fingerprint_enabled(&self, enabled: bool) {
        self.account_fingerprint_enabled
            .store(enabled, Ordering::Relaxed);
    }

    #[must_use]
    pub fn account_fingerprint_enabled(&self) -> bool {
        self.account_fingerprint_enabled.load(Ordering::Relaxed)
    }

    /// 开关开启时返回账号自己的 Desktop 身份；关闭或该平台暂无可用版本时返回 `None`，
    /// 调用方继续使用全局或 Key 覆盖身份，请求不因此失败。
    ///
    /// 必须在 Provider 共享的状态上调用：macOS 版本读取共享画像中已核验的官方最新版。
    #[must_use]
    pub fn account_profile(&self, installation_id: &str) -> Option<CodexWireProfile> {
        if !self.account_fingerprint_enabled() {
            return None;
        }
        match account_selection(installation_id).resolve(self) {
            Ok(profile) => Some(profile),
            Err(error) => {
                // 不输出 installation_id 原值。
                tracing::warn!(
                    error = %error,
                    "OpenAI account fingerprint is unavailable; using the global profile"
                );
                None
            }
        }
    }
}
