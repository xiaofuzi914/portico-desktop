//! Default [`NetworkPolicy`] implementation for outbound network access.

use crate::{NetworkPolicy, PermissionResult};
use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

/// Network policy with host/provider allowlists and private-network blocking.
#[derive(Debug, Clone)]
pub struct DefaultNetworkPolicy {
    allowed_hosts: HashSet<String>,
    allowed_providers: HashSet<String>,
    request_timeout: Duration,
    max_body_size: usize,
}

impl Default for DefaultNetworkPolicy {
    fn default() -> Self {
        Self {
            allowed_hosts: HashSet::new(),
            allowed_providers: HashSet::new(),
            request_timeout: Duration::from_secs(30),
            max_body_size: 10 * 1024 * 1024,
        }
    }
}

impl DefaultNetworkPolicy {
    /// Create a permissive policy that only blocks private/local networks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow a specific host (with or without port) for web fetch requests.
    pub fn allow_host(&mut self, host: impl Into<String>) {
        self.allowed_hosts.insert(host.into());
    }

    /// Allow a model provider host.
    pub fn allow_provider(&mut self, provider: impl Into<String>) {
        self.allowed_providers.insert(provider.into());
    }

    /// Set the request timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set the maximum allowed response body size in bytes.
    #[must_use]
    pub const fn with_max_body_size(mut self, size: usize) -> Self {
        self.max_body_size = size;
        self
    }

    /// Access the configured request timeout.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Access the configured maximum body size.
    #[must_use]
    pub const fn max_body_size(&self) -> usize {
        self.max_body_size
    }

    /// Check whether `host` has been explicitly allowed.
    fn is_allowed_host(&self, host: &str) -> bool {
        if self.allowed_hosts.contains(host) {
            return true;
        }
        if let Some(host_without_port) = host.split_once(':').map(|(h, _)| h) {
            if self.allowed_hosts.contains(host_without_port) {
                return true;
            }
        }
        false
    }
}

impl NetworkPolicy for DefaultNetworkPolicy {
    fn allow_request(&self, host: &str, port: u16) -> PermissionResult {
        let normalized = if host.contains(':') {
            host.to_owned()
        } else {
            format!("{host}:{port}")
        };

        if self.is_allowed_host(&normalized) || self.allowed_providers.contains(host) {
            return PermissionResult::Allowed;
        }

        if is_localhost_or_private(host) {
            return PermissionResult::Denied {
                reason: format!("{host} is a private/local address and is blocked by default"),
            };
        }

        PermissionResult::Allowed
    }
}

/// Determine whether a host string resolves to localhost or a private IP range.
fn is_localhost_or_private(host: &str) -> bool {
    let host_lower = host.to_lowercase();
    if host_lower == "localhost" || host_lower == "127.0.0.1" || host_lower == "::1" {
        return true;
    }

    if let Ok(addr) = host.parse::<IpAddr>() {
        return is_private_address(&addr);
    }

    false
}

const fn is_private_address(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(ipv4) => ipv4.is_loopback() || ipv4.is_private() || ipv4.is_link_local(),
        IpAddr::V6(ipv6) => ipv6.is_loopback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_public_host() {
        let policy = DefaultNetworkPolicy::new();
        assert!(matches!(
            policy.allow_request("example.com", 443),
            PermissionResult::Allowed
        ));
    }

    #[test]
    fn denies_localhost() {
        let policy = DefaultNetworkPolicy::new();
        assert!(matches!(
            policy.allow_request("localhost", 8080),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn denies_private_ip() {
        let policy = DefaultNetworkPolicy::new();
        assert!(matches!(
            policy.allow_request("192.168.1.10", 80),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn allowed_host_overrides_private_block() {
        let mut policy = DefaultNetworkPolicy::new();
        policy.allow_host("localhost:8080");
        assert!(matches!(
            policy.allow_request("localhost", 8080),
            PermissionResult::Allowed
        ));
    }

    #[test]
    fn allowed_provider_overrides_private_block() {
        let mut policy = DefaultNetworkPolicy::new();
        policy.allow_provider("ollama.local");
        assert!(matches!(
            policy.allow_request("ollama.local", 11434),
            PermissionResult::Allowed
        ));
    }

    #[test]
    fn config_values_are_accessible() {
        let policy = DefaultNetworkPolicy::new()
            .with_timeout(Duration::from_secs(60))
            .with_max_body_size(1024);
        assert_eq!(policy.request_timeout(), Duration::from_secs(60));
        assert_eq!(policy.max_body_size(), 1024);
    }
}
