//! The producer's keyspace-v2 registry slice, served verbatim over
//! `@rpc/tc/introspect` (RFC keyspace-v2 08 §6).
//!
//! Generic bus tooling — `zenctl topic list/info`, `zenctl doctor` — reads this
//! at runtime instead of a compiled-in table, which is what makes the
//! convention genuinely multi-application. The telemetry surface is enumerated
//! explicitly (two families), never a `{metric…}` catch-all: a catch-all makes
//! the lint vacuous and tells a consumer nothing about what a producer emits
//! (ZenSight's #468). `alive` is intentionally absent — it is a liveliness
//! token leaf, not a data subject (08 §2 reserves it).
//!
//! `path`s are relative to `<origin>/<class>/tc/`. Procedures carry the `fanout`
//! field (amendment G2): every `write` is `forbidden`, so a `*`-origin call is
//! refused; the read procedures are `allowed`.

/// The registry slice in the `zensight-keyspace` TOML format that `parse_slice`
/// accepts, so `zenctl --base tcgui` can consume it unmodified.
pub const REGISTRY_TOML: &str = r#"# tcgui producer registry (keyspace-v2). Served on @rpc/tc/introspect.
[registry]
version = "1.0"
app = "tcgui"
convention = 1

[producer]
name = "tc"
description = "Linux tc/netem traffic-control actuator (one per host)"

[[subject]]
path = "health"
class = "state"
type = "BackendHealthStatus"
cardinality = 1
since = "1.0"
description = "backend health document { host_id, name, status, ... }"

[[subject]]
path = "sensor"
class = "state"
type = "SensorDoc"
cardinality = 1
since = "1.0"
description = "producer registration: version + instance -> netns binding"

[[subject]]
path = "interface/{ns}/{iface}"
class = "state"
type = "NetworkInterface"
cardinality = 1024
since = "1.0"
description = "per-interface record; delete = NIC gone, up:false = disabled"

[[subject]]
path = "config/{ns}/{iface}"
class = "state"
type = "TcConfigUpdate"
cardinality = 1024
since = "1.0"
description = "applied TC/netem config echo; delete = cleared"

[[subject]]
path = "execution/{ns}/{iface}"
class = "state"
type = "ScenarioExecutionUpdate"
cardinality = 1024
since = "1.0"
description = "scenario execution status, keyed by interface (never a run-id)"

[[subject]]
path = "scenario/{id}"
class = "state"
type = "NetworkScenario"
cardinality = 1000
since = "1.0"
description = "scenario library entry; delete = removed"

[[subject]]
path = "preset/{id}"
class = "state"
type = "CustomPreset"
cardinality = 1000
since = "1.0"
description = "preset library entry; delete = removed"

[[subject]]
path = "bandwidth/{ns}/{iface}"
class = "telemetry"
type = "BandwidthUpdate"
cardinality = 1024
since = "1.0"
description = "per-interface bandwidth samples"

[[subject]]
path = "qdisc/{ns}/{iface}"
class = "telemetry"
type = "TcStatisticsUpdate"
cardinality = 1024
since = "1.0"
description = "per-interface qdisc/netem statistics"

[[subject]]
path = "applied/{ulid}"
class = "events"
type = "TcAppliedEvent"
cardinality = 0
since = "1.0"
description = "immutable audit record of a TC apply"

[[procedure]]
path = "config/{ns}/{iface}/set"
kind = "write"
fanout = "forbidden"
reply = "TcResponse"
idempotent = false
since = "1.0"
description = "apply or clear TC/netem on one interface"

[[procedure]]
path = "interface/{ns}/{iface}/set"
kind = "write"
fanout = "forbidden"
reply = "InterfaceControlResponse"
idempotent = false
since = "1.0"
description = "enable or disable one interface"

[[procedure]]
path = "scenario/set"
kind = "write"
fanout = "forbidden"
reply = "ScenarioResponse"
idempotent = false
since = "1.0"
description = "scenario CRUD (add/remove/update/get/list)"

[[procedure]]
path = "execution/{ns}/{iface}/set"
kind = "write"
fanout = "forbidden"
reply = "ScenarioExecutionResponse"
idempotent = false
since = "1.0"
description = "start/stop/pause/resume a scenario on one interface"

[[procedure]]
path = "diagnostics"
kind = "read"
fanout = "allowed"
reply = "DiagnosticsResponse"
idempotent = true
since = "1.0"
description = "run interface/TC diagnostics"

[[procedure]]
path = "introspect"
kind = "read"
fanout = "allowed"
reply = "toml"
idempotent = true
since = "1.0"
description = "return this registry slice as TOML (RFC 08 §6)"
"#;

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
