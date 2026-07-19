//! Host origin identity for the keyspace-v2 grammar, on `zenkey`.
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
//! distinguished as producer *instances* (`tc`, `tc-2`) instead. Minting is
//! `zenkey`'s reference derivation (RFC 06 §1): SHA-256 of the machine id with
//! tcgui's application salt — byte-identical to the pre-zenkey hand-rolled
//! derivation, so adopting zenkey did not re-key existing fleets.
//!
//! The `Local`/`Remote` split makes the write-safety rule a *type* (RFC 08 §1.1,
//! hardened to a MUST for writers by amendment G5): a backend mints exactly one
//! [`LocalOrigin`] and publishes under it; the frontend only ever holds
//! [`RemoteOrigin`]s learned from health documents and builds write keys from
//! them — you cannot format a write key from a name you typed or a wildcard.

use zenkey::AppProfile;

/// tcgui's application profile (RFC 06 §1): the app name and the origin salt.
/// The salt is the same domain-separation tag the pre-zenkey derivation used,
/// so origins are stable across the migration. Changing it re-keys every fleet.
pub static PROFILE: AppProfile = AppProfile::new("tcgui", "tcgui-host-id-v1");

/// Stable per-host machine-id sources, in priority order.
const MACHINE_ID_PATHS: [&str; 2] = ["/etc/machine-id", "/var/lib/dbus/machine-id"];

/// The typed origins are zenkey's own since 0.3 (RFC 08 §1.1/§1.2): the
/// `Local`/`Remote` split is a *type* — a backend mints exactly one
/// [`LocalOrigin`] and publishes under it; the frontend only ever holds
/// [`RemoteOrigin`]s parsed from wire data and builds call keys from them.
/// A `*` fleet selector implements neither, which is what keeps a fan-out
/// write unspellable (G2/G5).
pub use zenkey::origin::{ConcreteOrigin, HostOrigin, LocalOrigin, RemoteOrigin};

/// Mint this host's origin from the richest stable seed available: the
/// machine id, falling back to a persisted random id, falling back to the
/// hostname. The ladder predates zenkey's built-in minting and additionally
/// checks `/var/lib/dbus/machine-id` — kept so existing fallback hosts do
/// not re-key.
pub fn mint_local_origin() -> LocalOrigin {
    LocalOrigin::from_seed(&read_stable_host_seed(), PROFILE.salt())
}

/// Construct a local origin from an explicit seed (tests, or an
/// operator-provided id), with tcgui's salt.
pub fn local_origin_from_seed(seed: &str) -> LocalOrigin {
    LocalOrigin::from_seed(seed, PROFILE.salt())
}

/// Read a stable per-host seed for the origin hash: the machine id if available,
/// otherwise a persisted random id, otherwise the hostname (last resort).
///
/// This ladder predates zenkey's `AppProfile::host_id()` and additionally
/// checks `/var/lib/dbus/machine-id` — kept so existing fallback hosts do not
/// re-key.
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
        .join(PROFILE.app());
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
        let o = local_origin_from_seed("abcdef0123456789");
        let s = o.chunk();
        assert!(s.starts_with("h-"));
        assert_eq!(s.len(), 14);
        assert!(zenkey::grammar::is_valid_host_origin(s));
    }

    #[test]
    fn derivation_is_stable_and_seed_sensitive() {
        assert_eq!(
            local_origin_from_seed("machine-a").chunk(),
            local_origin_from_seed("machine-a").chunk()
        );
        assert_ne!(
            local_origin_from_seed("machine-a").chunk(),
            local_origin_from_seed("machine-b").chunk()
        );
        // Trimming: trailing newline (as read from /etc/machine-id) is ignored.
        assert_eq!(
            local_origin_from_seed("machine-a").chunk(),
            local_origin_from_seed("machine-a\n").chunk()
        );
    }

    /// The zenkey adoption must not re-key fleets: zenkey's reference
    /// derivation with tcgui's salt reproduces the pre-zenkey hand-rolled
    /// value (sha256(machine_id ++ "tcgui-host-id-v1"), first 6 bytes).
    #[test]
    fn derivation_matches_pre_zenkey_values() {
        use sha2::{Digest, Sha256};
        let machine_id = "b642b4217b34b1e8d3bd915fc65c4452";
        let mut hasher = Sha256::new();
        hasher.update(machine_id.as_bytes());
        hasher.update("tcgui-host-id-v1".as_bytes());
        let digest = hasher.finalize();
        let legacy = format!(
            "h-{}",
            digest.iter().take(6).fold(String::new(), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            })
        );
        assert_eq!(local_origin_from_seed(machine_id).chunk(), legacy);
    }

    /// A RemoteOrigin is always one concrete host — wildcards and junk are
    /// rejected at parse (0.3: Result, was Option), which is what keeps a
    /// fan-out write unspellable (G2).
    #[test]
    fn remote_origin_parse_rejects_junk_and_wildcards() {
        let good = local_origin_from_seed("x").chunk().to_string();
        assert!(RemoteOrigin::parse(&good).is_ok());
        assert!(RemoteOrigin::parse("*").is_err());
        assert!(RemoteOrigin::parse("h-XYZ").is_err());
        assert!(RemoteOrigin::parse("lab-router").is_err());
        assert!(RemoteOrigin::parse("h-3fa9c2d41b7").is_err()); // 11 hex
    }
}
