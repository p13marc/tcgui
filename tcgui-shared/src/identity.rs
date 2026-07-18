//! Host origin identity for the keyspace-v2 grammar.
//!
//! Every key is `tcgui/v1/<origin>/<class>/<producer>/<subject…>`. The `<origin>`
//! answers *who published it* with a **stable, opaque host id** — `h-<12 hex>` —
//! so hostnames stay in the payload (correctable without re-keying) and every
//! selector, ACL rule and drill-down is keyed on identity rather than a mutable
//! operator-chosen name (RFC keyspace-v2 06 §1, and its #474 post-mortem — the
//! reference GUI keyed its table on the payload hostname and every drill-down
//! broke in the multi-machine deployment, which is exactly tcgui's).
//!
//! tcgui is **host-as-origin, not container-as-origin**: the actuated resource
//! is one kernel's qdiscs, so N backends on a host share one origin and are
//! distinguished as producer *instances* (`tc`, `tc-2`) instead. The id is a
//! SHA-256 of the machine id with a domain-separation tag, so it is stable
//! across restarts and reveals nothing about the machine.
//!
//! The `Local`/`Remote` split makes the write-safety rule a *type* (RFC 08 §1.1,
//! hardened to a MUST for writers by amendment G5): a backend mints exactly one
//! [`LocalOrigin`] and publishes under it; the frontend only ever holds
//! [`RemoteOrigin`]s learned from health documents and builds write keys from
//! them — you cannot format a write key from a name you typed or a wildcard.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Domain-separation tag mixed into the host-id hash so the value is unique to
/// tcgui and cannot be correlated with another consumer of the same machine id.
const HOST_ID_DOMAIN: &str = "tcgui-host-id-v1";

/// Stable per-host machine-id sources, in priority order.
const MACHINE_ID_PATHS: [&str; 2] = ["/etc/machine-id", "/var/lib/dbus/machine-id"];

/// An opaque, stable host origin: `h-<12 lowercase hex>`.
///
/// This is the *value* that occupies the origin chunk. It is deliberately not
/// directly constructible from an arbitrary string in publishing code — mint a
/// [`LocalOrigin`] (this host) or parse a [`RemoteOrigin`] (a peer) instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Origin(String);

impl Origin {
    /// The origin chunk as it appears in a key (`h-3fa9c2d41b7e`).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Derive a host origin from a raw machine-id string.
    fn from_machine_id(machine_id: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(machine_id.trim().as_bytes());
        hasher.update(HOST_ID_DOMAIN.as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().take(6).fold(String::new(), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        });
        Origin(format!("h-{hex}"))
    }

    /// Validate that `s` is a well-formed concrete host origin (`h-<12 hex>`).
    fn is_valid_host(s: &str) -> bool {
        s.len() == 14
            && s.starts_with("h-")
            && s[2..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A **concrete** (never wildcard) origin usable to build an `@rpc` key. Both
/// [`LocalOrigin`] (a backend serving on its own key) and [`RemoteOrigin`] (the
/// frontend calling a specific host) implement it; a `*` fleet selector does
/// not — which is what makes a fan-out *write* unspellable at the type level
/// (amendments G2/G5).
pub trait ConcreteOrigin {
    /// The origin chunk to place in the key.
    fn as_key_chunk(&self) -> &str;
}

/// This host's own origin. A backend mints exactly one and publishes every
/// state/telemetry/event key under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOrigin(Origin);

impl LocalOrigin {
    /// Mint this host's origin from the system machine id, falling back to a
    /// persisted random id if no machine id is readable (containers, minimal
    /// images). The fallback is still *stable per host* — the invariant the
    /// origin must preserve — it just is not derivable from the machine id.
    pub fn mint() -> Self {
        LocalOrigin(Origin::from_machine_id(&read_stable_host_seed()))
    }

    /// Construct from an explicit seed (tests, or an operator-provided id).
    pub fn from_seed(seed: &str) -> Self {
        LocalOrigin(Origin::from_machine_id(seed))
    }

    /// Borrow the underlying origin (for key building and the health document).
    pub fn origin(&self) -> &Origin {
        &self.0
    }

    /// The origin chunk as it appears in a key.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl ConcreteOrigin for LocalOrigin {
    fn as_key_chunk(&self) -> &str {
        self.0.as_str()
    }
}

/// An origin learned from a peer's health document. The frontend builds write
/// (`@rpc`) keys from these and never from a name it displayed — this is the
/// identity bridge (06 §6): display the payload `name`, key on the `host_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteOrigin(Origin);

impl RemoteOrigin {
    /// Parse a concrete host origin received over the wire. Returns `None` for a
    /// malformed value or a wildcard — a `RemoteOrigin` is always a single
    /// concrete host, which is what keeps a fan-out *write* unspellable (G2).
    pub fn parse(s: &str) -> Option<Self> {
        Origin::is_valid_host(s).then(|| RemoteOrigin(Origin(s.to_string())))
    }

    /// The origin chunk as it appears in a key.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl ConcreteOrigin for RemoteOrigin {
    fn as_key_chunk(&self) -> &str {
        self.0.as_str()
    }
}

/// Read a stable per-host seed for the origin hash: the machine id if available,
/// otherwise a persisted random id, otherwise the hostname (last resort).
fn read_stable_host_seed() -> String {
    for path in MACHINE_ID_PATHS {
        if let Ok(contents) = std::fs::read_to_string(path) {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    if let Some(persisted) = read_or_create_fallback_id() {
        return persisted;
    }
    // Absolute last resort: the hostname keeps the value stable per host.
    std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|_| "tcgui-unknown-host".to_string())
}

/// Read (or create) a persisted random host id under the runtime state dir.
fn read_or_create_fallback_id() -> Option<String> {
    let dir = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state"))
        })?
        .join("tcgui");
    let path = dir.join("host-id");

    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // Mint 16 bytes of randomness from the OS and persist them.
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .ok()?;
    let id: String = buf.iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    });
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, &id);
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_shape_is_h_plus_12_hex() {
        let o = LocalOrigin::from_seed("abcdef0123456789");
        let s = o.as_str();
        assert!(s.starts_with("h-"));
        assert_eq!(s.len(), 14);
        assert!(Origin::is_valid_host(s));
    }

    #[test]
    fn derivation_is_stable_and_seed_sensitive() {
        assert_eq!(
            LocalOrigin::from_seed("machine-a").as_str(),
            LocalOrigin::from_seed("machine-a").as_str()
        );
        assert_ne!(
            LocalOrigin::from_seed("machine-a").as_str(),
            LocalOrigin::from_seed("machine-b").as_str()
        );
        // Trimming: trailing newline (as read from /etc/machine-id) is ignored.
        assert_eq!(
            LocalOrigin::from_seed("machine-a").as_str(),
            LocalOrigin::from_seed("machine-a\n").as_str()
        );
    }

    #[test]
    fn remote_origin_parse_rejects_junk_and_wildcards() {
        let good = LocalOrigin::from_seed("x").as_str().to_string();
        assert!(RemoteOrigin::parse(&good).is_some());
        assert!(RemoteOrigin::parse("*").is_none());
        assert!(RemoteOrigin::parse("h-XYZ").is_none());
        assert!(RemoteOrigin::parse("lab-router").is_none());
        assert!(RemoteOrigin::parse("h-3fa9c2d41b7").is_none()); // 11 hex
    }
}
