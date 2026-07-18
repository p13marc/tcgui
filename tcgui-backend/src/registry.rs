//! The producer's keyspace-v2 registry slice, served verbatim over
//! `@rpc/tc/introspect` (RFC keyspace-v2 08 §6).
//!
//! The slice is the registry file `tcgui-shared/registry/tc.toml`, compiled by
//! `zenkey-build` (typed builders + this raw TOML) — one source of truth: the
//! keys the backend publishes and the slice it serves cannot drift apart.
//! Generic bus tooling — `zenctl topic list/info`, `zenctl doctor` — reads it
//! at runtime instead of a compiled-in table, which is what makes the
//! convention genuinely multi-application.

pub use tcgui_shared::registry::tc::REGISTRY_TOML;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_nonempty_and_names_the_producer() {
        assert!(REGISTRY_TOML.contains("name = \"tc\""));
        // Telemetry is enumerated, never a catch-all.
        assert!(REGISTRY_TOML.contains("bandwidth/{ns}/{iface}"));
        assert!(!REGISTRY_TOML.contains("{metric"));
        // Writes are fan-out-forbidden (amendment G2).
        assert!(REGISTRY_TOML.contains("fanout = \"forbidden\""));
        // alive is not a data subject.
        assert!(!REGISTRY_TOML.contains("path = \"alive\""));
    }
}
