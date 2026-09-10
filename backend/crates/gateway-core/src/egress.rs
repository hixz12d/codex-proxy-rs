//! Account-scoped egress facts. Proxy credentials never participate in Debug output.

use std::fmt;

/// Validated SOCKS5 endpoint, including optional credentials.
#[derive(Clone, PartialEq, Eq)]
pub struct ProxyEndpoint(String);

impl ProxyEndpoint {
    pub fn parse(value: &str) -> Result<Self, ProxyEndpointError> {
        if value.len() > 2048 || value.chars().any(char::is_control) || value.trim() != value {
            return Err(ProxyEndpointError);
        }
        let url = url::Url::parse(value).map_err(|_| ProxyEndpointError)?;
        if !matches!(url.scheme(), "socks5" | "socks5h")
            || url.host_str().is_none()
            || url.port().is_none_or(|port| port == 0)
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || (url.password().is_some() && url.username().is_empty())
        {
            return Err(ProxyEndpointError);
        }
        for part in [url.username(), url.password().unwrap_or("")] {
            let bytes = part.as_bytes();
            if bytes.iter().enumerate().any(|(i, byte)| {
                *byte == b'%'
                    && (bytes.get(i + 1).is_none_or(|b| !b.is_ascii_hexdigit())
                        || bytes.get(i + 2).is_none_or(|b| !b.is_ascii_hexdigit()))
            }) {
                return Err(ProxyEndpointError);
            }
            let decoded = percent_encoding::percent_decode_str(part)
                .decode_utf8()
                .map_err(|_| ProxyEndpointError)?;
            if decoded.len() > 255 || decoded.chars().any(char::is_control) {
                return Err(ProxyEndpointError);
            }
        }
        Ok(Self(url.to_string()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Safe display value, with both username and password removed.
    pub fn address(&self) -> String {
        let mut url = url::Url::parse(&self.0).expect("validated proxy endpoint");
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.to_string()
    }
}

impl fmt::Debug for ProxyEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProxyEndpoint([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid SOCKS5 endpoint")]
pub struct ProxyEndpointError;

/// A frozen route for one account attempt; key includes binding and node revisions.
#[derive(Debug, Clone)]
pub struct AccountProxyRoute {
    pub key: String,
    pub endpoint: ProxyEndpoint,
}
