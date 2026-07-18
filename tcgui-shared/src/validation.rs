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

/// Reserved prefix marking an escaped key chunk. Chosen so it begins with an
/// alphanumeric (`0`) — the G4 erratum requirement that the escape can never
/// regress into another non-alphanumeric leading character — while being a
/// two-character sequence that a *clean* chunk is forbidden from starting with,
/// which is what makes the whole mapping injective (see [`slug_key_chunk`]).
const ESC_PREFIX: &str = "0_";

/// A character that may appear verbatim inside a single key-expression chunk as
/// tcgui uses them: the interface/namespace charset (`[A-Za-z0-9._:@-]`) that
/// Zenoh already accepts mid-chunk. Everything else — the Zenoh-reserved
/// `* ? # $`, the `/` separator, whitespace and control bytes — must be escaped.
fn is_chunk_literal(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '@')
}

/// A chunk is *clean* — reproducible verbatim, no wire change — when it starts
/// with an alphanumeric, contains only [`is_chunk_literal`] characters, and does
/// not intrude on the reserved [`ESC_PREFIX`] namespace.
fn is_clean_chunk(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && !name.starts_with(ESC_PREFIX)
        && name.chars().all(is_chunk_literal)
}

/// Turn an arbitrary, possibly hostile name (an interface or namespace straight
/// off netlink) into exactly one safe Zenoh key-expression chunk.
///
/// The backend runs `CAP_NET_ADMIN` and builds key expressions from names it
/// *discovers*, not from validated requests. Linux `dev_valid_name()` permits
/// `*`, `**`, `?`, `#`, `$` in an interface name, and those reach `keformat!`:
/// `?`/`#`/`$`/`**` make the format call fail (panicking a privileged daemon
/// behind `.expect()`), while `*` silently *succeeds* and makes the daemon
/// declare a publisher on a **wildcard** key. This function closes both holes.
///
/// Design (RFC keyspace-v2 03 §2, with the G4 erratum):
/// - **Clean names pass through unchanged** — `eth0`, `eth0.100`, `eth0:0`,
///   `veth@if5`, `default`, `myns`, `ETH0` — so no key that works today changes
///   on the wire, and interface-name case is preserved (`eth0` ≠ `ETH0`,
///   injectivity over the lowercasing MUST — the G4 case-sensitivity carve-out).
/// - Any other name is emitted as `ESC_PREFIX` + a body in which each
///   non-literal byte becomes `_xNN_` (lowercase hex). The alphanumeric-leading
///   `ESC_PREFIX` guarantees a conforming first character (no `_myns`-style
///   infinite regress) and, because clean chunks may never begin with it, keeps
///   the mapping injective: `starts_with(ESC_PREFIX)` alone decides which side a
///   chunk came from.
///
/// Total: never panics, never returns an empty string, never returns a chunk
/// containing `/` or a Zenoh wildcard.
pub fn slug_key_chunk(name: &str) -> String {
    if is_clean_chunk(name) {
        return name.to_string();
    }
    let mut out = String::with_capacity(name.len() + ESC_PREFIX.len());
    out.push_str(ESC_PREFIX);
    for &b in name.as_bytes() {
        let c = b as char;
        if is_chunk_literal(c) {
            out.push(c);
        } else {
            out.push_str(&format!("_x{b:02x}_"));
        }
    }
    out
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

    #[test]
    fn slug_passes_clean_names_unchanged() {
        // Every name that works on the wire today must be byte-identical after
        // slugging — this is the "no wire change" guarantee.
        for name in [
            "eth0", "eth0.100", "eth0:0", "veth@if5", "default", "myns", "lo",
        ] {
            assert_eq!(slug_key_chunk(name), name, "clean name {name:?} changed");
        }
    }

    #[test]
    fn slug_preserves_interface_case() {
        // G4 case-sensitivity carve-out: eth0 and ETH0 are two different NICs
        // and must never collide (injectivity beats the lowercasing rule).
        assert_eq!(slug_key_chunk("ETH0"), "ETH0");
        assert_ne!(slug_key_chunk("eth0"), slug_key_chunk("ETH0"));
    }

    #[test]
    fn slug_neutralizes_zenoh_hostile_names() {
        // The names that today panic the daemon or make it publish on a
        // wildcard key. None may come back containing a wildcard or separator.
        for name in ["*", "**", "?", "#", "$x", "a/b", "has space"] {
            let s = slug_key_chunk(name);
            assert!(!s.is_empty());
            assert!(!s.contains(['/', '*', '?', '#', '$']), "{name:?} -> {s:?}");
            assert!(s.as_bytes()[0].is_ascii_alphanumeric(), "{name:?} -> {s:?}");
        }
        // `*` must NOT round-trip to a wildcard chunk.
        assert_ne!(slug_key_chunk("*"), "*");
    }

    #[test]
    fn slug_converges_on_leading_underscore() {
        // The G4 infinite-regress case: `_myns` is not charset-legal (leading
        // `_`); the escaped form must still start alphanumeric.
        let s = slug_key_chunk("_myns");
        assert!(s.as_bytes()[0].is_ascii_alphanumeric(), "{s:?}");
        assert_ne!(s, "_myns");
    }

    #[test]
    fn slug_is_injective_on_tricky_pairs() {
        // The reserved-prefix namespace keeps escaped forms from colliding with
        // clean names that happen to look like an escape.
        let cases = ["*", "eth0", "_myns", "0_foo", "a_x2a_b", "ETH0", "eth0"];
        for a in cases {
            for b in cases {
                assert_eq!(
                    a == b,
                    slug_key_chunk(a) == slug_key_chunk(b),
                    "injectivity broken for {a:?} vs {b:?}"
                );
            }
        }
    }
}
