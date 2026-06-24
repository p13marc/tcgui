//! Validation for untrusted request inputs (namespace / interface names,
//! payload sizes).
//!
//! The backend runs with `CAP_NET_ADMIN` and accepts requests over Zenoh.
//! Namespace names flow into filesystem paths (`/var/run/netns/<name>`) and
//! interface names into privileged netlink operations, so they must be
//! validated against a strict allowlist before use — defense-in-depth for the
//! privileged daemon, independent of the Zenoh-network trust boundary.

/// Maximum accepted size for a single Zenoh request payload (64 KiB).
///
/// Generous for any legitimate TC/interface/diagnostics request, but bounds the
/// memory a single malformed/hostile request can force us to buffer + parse.
pub const MAX_REQUEST_PAYLOAD_BYTES: usize = 64 * 1024;

/// Maximum number of steps accepted in a single scenario.
pub const MAX_SCENARIO_STEPS: usize = 10_000;

/// Linux `IFNAMSIZ` is 16, so interface names are at most 15 bytes.
const MAX_IFNAME_LEN: usize = 15;

/// Upper bound on a namespace token length (well above any real netns name).
const MAX_NS_TOKEN_LEN: usize = 255;

/// A name segment is safe if it is a non-empty, non-`.`/`..` token containing
/// only `[A-Za-z0-9._-]` (plus the extra chars in `extra`), with no `/`,
/// whitespace, or control characters — so it can never escape a path or carry
/// shell/netlink surprises.
fn is_safe_token(token: &str, extra: &[char]) -> bool {
    if token.is_empty() || token == "." || token == ".." {
        return false;
    }
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') || extra.contains(&c))
}

/// Validate a namespace identifier as used by tcgui: `"default"`, a named netns
/// (`/var/run/netns/<name>`), or a container namespace (`"container:<name>"`).
pub fn validate_namespace(namespace: &str) -> Result<(), String> {
    if namespace == "default" {
        return Ok(());
    }
    if namespace.len() > MAX_NS_TOKEN_LEN + "container:".len() {
        return Err(format!(
            "namespace name too long: {} bytes",
            namespace.len()
        ));
    }

    // Container namespaces are "container:<name>"; validate the suffix.
    let token = namespace.strip_prefix("container:").unwrap_or(namespace);

    if !is_safe_token(token, &[]) {
        return Err(format!(
            "invalid namespace name {namespace:?}: expected 'default', a netns name, \
             or 'container:<name>' using only [A-Za-z0-9._-]"
        ));
    }
    Ok(())
}

/// Validate a Linux interface name: 1..=15 bytes, no `/`, whitespace, or control
/// characters, and not `.`/`..`.
pub fn validate_interface(interface: &str) -> Result<(), String> {
    if interface.len() > MAX_IFNAME_LEN {
        return Err(format!(
            "interface name {interface:?} too long ({} > {MAX_IFNAME_LEN})",
            interface.len()
        ));
    }
    // Allow the punctuation that appears in real interface names: VLANs
    // (`eth0.100`), aliases (`eth0:0`), and the `@` veth peers can show.
    if !is_safe_token(interface, &[':', '@']) {
        return Err(format!(
            "invalid interface name {interface:?}: expected a Linux interface name \
             (1..=15 chars, [A-Za-z0-9._:@-])"
        ));
    }
    Ok(())
}

/// Validate a `(namespace, interface)` request target.
pub fn validate_target(namespace: &str, interface: &str) -> Result<(), String> {
    validate_namespace(namespace)?;
    validate_interface(interface)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_targets() {
        assert!(validate_target("default", "eth0").is_ok());
        assert!(validate_target("myns", "eth0.100").is_ok());
        assert!(validate_target("container:nginx", "veth1234").is_ok());
        assert!(validate_interface("eth0:0").is_ok());
    }

    #[test]
    fn rejects_path_traversal_namespace() {
        assert!(validate_namespace("../../etc").is_err());
        assert!(validate_namespace("..").is_err());
        assert!(validate_namespace("foo/bar").is_err());
        assert!(validate_namespace("container:../escape").is_err());
        assert!(validate_namespace("").is_err());
    }

    #[test]
    fn rejects_bad_interface() {
        assert!(validate_interface("").is_err());
        assert!(validate_interface("eth/0").is_err());
        assert!(validate_interface("has space").is_err());
        assert!(validate_interface("toolonginterfacename").is_err()); // > 15
        assert!(validate_interface("..").is_err());
    }
}
