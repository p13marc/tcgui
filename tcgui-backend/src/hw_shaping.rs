//! Hardware TX shaping (`net_shaper`) capability detection.
//!
//! Read-only probe over nlink's `Connection<NetShaper>` (the kernel 6.13+
//! `net_shaper` Generic Netlink family). It detects whether the running kernel
//! and NIC drivers can offload rate limiting to hardware on shaper-capable NICs
//! (e.g. Intel `ice`, Mellanox `mlx5`, Broadcom `bnxt`).
//!
//! # Scope: detection only
//!
//! This module deliberately implements **only** capability detection — it never
//! installs or removes a hardware shaper (`set_shaper`/`del_shaper`). The apply
//! path, and any frontend surface, are intentionally deferred: hardware TX
//! shaping cannot be exercised without a shaper-capable NIC and has no CI
//! coverage, so shipping a blind apply path would be untestable. See
//! `docs/net-shaper-hw-shaping.md` for the full design and the rollout plan.
//!
//! The probe is best-effort and fully graceful: a kernel without the family
//! (anything < 6.13), a driver that doesn't implement `net_shaper`, or missing
//! privileges all degrade to "not supported" rather than surfacing an error.

use nlink::netlink::Connection;
use nlink::netlink::genl::net_shaper::{NetShaper, NetShaperScope};
use tracing::debug;

/// Probe the given interfaces for hardware TX rate-limit support.
///
/// `interfaces` is a slice of `(ifindex, name)` for the default-namespace
/// interfaces to check (`net_shaper` operates on physical NICs in the host
/// netns). Returns the names of interfaces whose driver advertises `bw_max`
/// (the capability that backs hardware rate limiting).
///
/// Read-only and infallible by design: every failure path logs at `debug` and
/// is treated as "unsupported".
pub async fn probe(interfaces: &[(u32, String)]) -> Vec<String> {
    // `new_async` resolves the GENL family id. A failure here almost always
    // means the family does not exist (kernel < 6.13) — not a real error.
    let conn = match Connection::<NetShaper>::new_async().await {
        Ok(conn) => conn,
        Err(e) => {
            debug!("Hardware TX shaping (net_shaper) unavailable: {e}");
            return Vec::new();
        }
    };

    let mut capable = Vec::new();
    for (ifindex, name) in interfaces {
        // Query the netdev-scope caps (the whole-interface shaper root).
        match conn.get_caps(*ifindex, NetShaperScope::Netdev).await {
            Ok(caps) if caps.support_bw_max => capable.push(name.clone()),
            Ok(_) => {}
            // Most drivers don't implement net_shaper and return EOPNOTSUPP;
            // that's expected, so this stays at debug.
            Err(e) => debug!("net_shaper caps probe failed for {name}: {e}"),
        }
    }

    if capable.is_empty() {
        debug!("net_shaper family present but no interface supports HW rate limiting");
    }
    capable
}
