# Hardware TX shaping (`net_shaper`) — design & rollout

Status: **Phase 1 (detection) implemented; apply path deferred pending hardware.**
Tracking issue: #7.

## Motivation

Today every rate limit tcgui applies goes through **netem `rate`**
(`tc_commands.rs`, `NetemConfig::rate`), which is *software* shaping in the
qdisc layer. nlink 0.16 added a typed `Connection<NetShaper>` API for the Linux
**`net_shaper`** Generic Netlink family (kernel 6.13+), which lets capable NICs
offload rate limiting (and burst / priority / weight) to **hardware**:

- Lower CPU — pacing happens on the NIC, not in the qdisc softirq.
- More accurate pacing at high rates.
- Composes with netem: keep loss / delay / corruption emulation in netem while
  the NIC enforces the bandwidth cap.

Shaper-capable drivers today: Intel `ice` (E810/E830), Mellanox `mlx5`
(ConnectX-7+), Broadcom `bnxt`.

## nlink API (0.21)

```rust
use nlink::netlink::Connection;
use nlink::netlink::genl::net_shaper::{NetShaper, NetShaperScope, NetShaperSetRequest};

let conn = Connection::<NetShaper>::new_async().await?;     // resolves GENL family (kernel 6.13+)

// Capability query (read-only):
let caps = conn.get_caps(ifindex, NetShaperScope::Netdev).await?;
//   caps.support_bw_max / support_bw_min / support_burst / support_priority / ...

// Apply / remove (CAP_NET_ADMIN):
conn.set_shaper(NetShaperSetRequest::new(ifindex, handle)./* bw_max/metric/... */).await?;
conn.del_shaper(ifindex, handle).await?;
```

`NetShaperScope`: `Netdev` (whole interface), `Queue` (one TX queue), `Node`
(intermediate scheduler node). For an interface-wide rate cap we use `Netdev`.

## Phase 1 — capability detection (implemented)

`tcgui-backend/src/hw_shaping.rs::probe()` runs once at startup against the
default-namespace interfaces:

- `Connection::<NetShaper>::new_async()` — a failure means the family is absent
  (kernel < 6.13); degrades to "unsupported".
- For each interface, `get_caps(ifindex, Netdev)`; an interface is
  rate-limit-capable iff `caps.support_bw_max`.
- Drivers without `net_shaper` return `EOPNOTSUPP`; treated as unsupported.

Everything is read-only and best-effort — no failure path is fatal. On typical
hardware (no shaper-capable NIC) the probe is a silent no-op; where a capable
NIC is present it logs the interface names.

## Phase 2 — apply path (deferred)

Blocked on access to shaper-capable hardware for validation (no CI coverage is
possible — nlink itself validates `net_shaper` only via its manual
hardware-validation checklist).

Planned shape:

1. **Shared types** (`tcgui-shared`): extend `TcRateLimitConfig` with an
   optional `hardware: bool` (or a `RateLimitBackend { Netem, Hardware }`
   enum). Default `Netem` preserves today's behavior and wire-compat.
2. **Backend** (`tc_commands.rs`): when `hardware` is requested **and** the
   interface advertised `support_bw_max`, apply via `set_shaper` instead of
   folding `rate` into the netem qdisc; remove via `del_shaper`. Fall back to
   netem (with a warning) when unsupported, so a config is never silently
   dropped.
3. **Capability surface**: carry per-interface `hw_shaping_capable` from the
   Phase 1 probe into `NetworkInterface` so the frontend can offer the hardware
   option only where it works.
4. **Frontend**: a "hardware rate limit" toggle on the `RateLimit` feature,
   enabled only for capable interfaces.

## Testing / validation constraints

- **No CI**: GitHub runners have no shaper-capable NICs. Phase 2 must be
  validated manually against real hardware (mirror nlink's
  `docs/release-validation-manual.md` convention).
- Phase 1 *is* exercisable everywhere: it must run cleanly and report
  "unsupported" on machines without the family or a capable driver.

## Why detection-only for now

Applying hardware shapers we cannot test risks mis-programming a NIC's TX path.
Detection is safe, proves the nlink integration compiles and runs against 0.21,
and is the foundation Phase 2 builds on once hardware is available.
