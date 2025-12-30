# Eliminating Process Spawning: Rust Crate Alternatives

**Date:** December 2025  
**Status:** Research Report

## Executive Summary

This report analyzes the external binaries currently spawned by tcgui-backend and evaluates available Rust crate alternatives. The goal is to eliminate process spawning dependencies in favor of native Rust implementations using netlink and system calls.

**Verdict:** Full elimination of process spawning is **not yet feasible** for all use cases. The critical blocker is the lack of a mature Rust crate for writing tc netem configurations. However, partial migration is possible for namespace handling and some network operations.

## Current External Binary Dependencies

| Binary | Usage | Spawning Locations |
|--------|-------|-------------------|
| `tc` | Traffic control qdisc add/replace/delete | `tc_commands.rs`, `network.rs` |
| `ip` | Network namespace operations, link state | `tc_commands.rs`, `network.rs`, `bandwidth.rs` |
| `nsenter` | Enter container network namespaces | `tc_commands.rs`, `container.rs`, `bandwidth.rs` |
| `docker`/`podman` | Container inspection (fallback) | `container.rs` |

### Detailed Usage Analysis

#### `tc` (traffic control)
- **Apply netem qdisc:** `tc qdisc add/replace dev <iface> root netem loss/delay/corrupt/...`
- **Remove qdisc:** `tc qdisc del dev <iface> root`
- **Query qdisc:** `tc qdisc show dev <iface>` (also used via netlink already for some cases)

#### `ip` (iproute2)
- **Namespace execution:** `ip netns exec <ns> <command>`
- **Namespace listing:** `ip netns list`
- **Link state changes:** `ip link set <iface> up/down`
- **Interface discovery in namespaces:** `ip -j link show` (JSON output)

#### `nsenter`
- **Container namespace entry:** `nsenter --net=/proc/<pid>/ns/net <command>`
- Used for containers where `ip netns exec` doesn't work

## Rust Crate Alternatives

### 1. Traffic Control (tc) - PARTIAL SUPPORT

#### [netlink-tc](https://crates.io/crates/netlink-tc) / [rust-tc](https://github.com/mmynk/rust-tc)
- **Version:** 0.0.4 (October 2023)
- **Status:** Early development, **read-only**
- **Supported qdiscs:** fq_codel, htb, clsact
- **netem support:** NO
- **Write operations:** NO (planned in roadmap)

#### [netlink-packet-route](https://crates.io/crates/netlink-packet-route)
- **tc module:** Yes, includes `TcMessage`, `TcQdiscFqCodel`, `TcQdiscIngress`
- **netem support:** NO - no netem structures defined
- **Write capability:** The underlying netlink structures exist, but no high-level API
- **Documentation:** Only 11% documented

#### [libbpf-rs](https://docs.rs/libbpf-rs/latest/libbpf_rs/struct.TcHook.html)
- **Purpose:** eBPF-based traffic control (clsact qdisc)
- **netem replacement:** NO - eBPF TC is for classification/filtering, not delay/loss simulation
- **Use case:** Different from netem (packet manipulation vs scheduling simulation)

#### Recommendation: TC

**Short term:** Continue using `tc` binary for netem operations. No Rust crate currently supports writing netem configurations.

**Long term options:**
1. **Wait for netlink-tc/rust-tc** to add write support and netem (no timeline)
2. **Implement netem netlink messages** using netlink-packet-route primitives (significant effort)
3. **Contribute netem support** to rust-netlink ecosystem

**Implementation effort for DIY netem:** High. Would require:
- Understanding kernel tc netlink protocol for netem
- Implementing `TcQdiscNetem` struct with all netem options (delay, loss, corrupt, reorder, rate, etc.)
- Building RTM_NEWQDISC/RTM_DELQDISC messages
- Testing against all kernel versions

Reference: [libnl netem source](https://www.infradead.org/~tgr/libnl/doc/api/netem_8c_source.html) shows the complexity involved.

---

### 2. Network Namespace Entry - GOOD SUPPORT

#### [nix](https://crates.io/crates/nix) (setns)
- **Function:** `nix::sched::setns(fd, CloneFlags::CLONE_NEWNET)`
- **Status:** Stable, well-maintained
- **Usage:** Direct syscall wrapper for entering namespaces
- **Example:**
  ```rust
  use nix::sched::{setns, CloneFlags};
  use std::fs::File;
  
  let ns_file = File::open("/proc/<pid>/ns/net")?;
  setns(&ns_file, CloneFlags::CLONE_NEWNET)?;
  // Now in target namespace
  ```

#### [netns-rs](https://crates.io/crates/netns-rs)
- **Repository:** [openanolis/netns-rs](https://github.com/openanolis/netns-rs)
- **License:** Apache-2.0
- **Status:** Minimal maintenance (11 commits total, created Feb 2022)
- **API:**
  ```rust
  use netns_rs::NetNs;
  
  let ns = NetNs::get("my_netns")?;
  ns.run(|_| {
      // Code runs in namespace
  })?;
  ```
- **Limitation:** Designed for traditional `/var/run/netns/` namespaces, not arbitrary `/proc/<pid>/ns/net` paths

#### Recommendation: Namespace Entry

**Use `nix::sched::setns`** directly. It's the most flexible approach:
- Works with any namespace path (containers, traditional netns)
- Well-maintained crate with wide adoption
- Direct syscall semantics, no abstraction overhead

**Pattern for container namespaces:**
```rust
use nix::sched::{setns, CloneFlags};
use std::fs::File;

fn run_in_namespace<F, T>(ns_path: &Path, f: F) -> Result<T>
where
    F: FnOnce() -> T,
{
    // Save current namespace
    let current_ns = File::open("/proc/self/ns/net")?;
    
    // Enter target namespace
    let target_ns = File::open(ns_path)?;
    setns(&target_ns, CloneFlags::CLONE_NEWNET)?;
    
    let result = f();
    
    // Return to original namespace
    setns(&current_ns, CloneFlags::CLONE_NEWNET)?;
    
    Ok(result)
}
```

**Caveat:** This changes the namespace for the current thread. For async code, you may need to spawn a blocking task or use thread-per-namespace patterns.

---

### 3. Network Interface Operations - ALREADY USING

#### [rtnetlink](https://crates.io/crates/rtnetlink)
- **Status:** Already a dependency in tcgui-backend
- **Capabilities:**
  - Link enumeration (equivalent to `ip link show`)
  - Link state changes (equivalent to `ip link set up/down`)
  - Address management
  - Route management
- **Usage in codebase:** Already used for default namespace interface discovery

#### Recommendation: Interface Operations

**Expand rtnetlink usage** to replace remaining `ip` commands:
- `ip link set <iface> up/down` → `rtnetlink::Handle::link().set(<idx>).up()/.down()`
- `ip -j link show` → Already partially implemented

**Challenge:** rtnetlink operates on the current thread's namespace. For cross-namespace operations, combine with `setns`.

---

### 4. Container Discovery - ALREADY NATIVE

#### [bollard](https://crates.io/crates/bollard)
- **Status:** Already a dependency (v0.18)
- **Purpose:** Docker/Podman API client
- **Usage:** Primary container discovery method

The `docker`/`podman` CLI fallback in `container.rs` is only for edge cases. The bollard crate handles most container operations natively.

---

## Migration Strategy

### Phase 1: Namespace Entry (Low Risk)
Replace `nsenter` and `ip netns exec` with `nix::sched::setns`:

1. Add `nix` crate with `sched` feature
2. Implement namespace-aware execution helper
3. Migrate container namespace operations
4. Migrate traditional namespace operations

**Estimated complexity:** Medium  
**Risk:** Low - well-understood syscall semantics

### Phase 2: Expand rtnetlink Usage (Low Risk)
Replace remaining `ip link` commands:

1. Use rtnetlink for link up/down in namespaces (with Phase 1 setns)
2. Use rtnetlink for interface enumeration in namespaces

**Estimated complexity:** Low  
**Risk:** Low - already using rtnetlink

### Phase 3: TC Operations (HIGH RISK - NOT RECOMMENDED NOW)
Options for eliminating `tc` binary:

**Option A: Wait for ecosystem maturity**
- Monitor rust-tc/netlink-tc for write support
- Reassess in 6-12 months

**Option B: Contribute to rust-netlink**
- Implement netem support in netlink-packet-route
- Significant effort, requires deep netlink knowledge
- Benefits the wider Rust ecosystem

**Option C: Custom implementation**
- Build netem netlink messages directly
- Use netlink-packet-route primitives
- High effort, maintenance burden

**Recommendation:** Wait for ecosystem maturity. The tc binary is stable and the netem interface rarely changes. Process spawning overhead is acceptable for the operation frequency (user-initiated TC changes).

---

## Crate Summary Table

| Purpose | Crate | Version | Maturity | Recommendation |
|---------|-------|---------|----------|----------------|
| Namespace entry | [nix](https://crates.io/crates/nix) | 0.30.x | Stable | USE |
| Namespace helpers | [netns-rs](https://crates.io/crates/netns-rs) | 0.1.0 | Low | SKIP (use nix directly) |
| Network links | [rtnetlink](https://crates.io/crates/rtnetlink) | 0.14.x | Stable | ALREADY USING |
| TC read-only | [netlink-tc](https://crates.io/crates/netlink-tc) | 0.0.4 | Alpha | NOT READY |
| TC write/netem | None available | - | - | WAIT |
| Container API | [bollard](https://crates.io/crates/bollard) | 0.18 | Stable | ALREADY USING |

---

## Conclusion

**Immediate wins (Phase 1-2):**
- Eliminate `nsenter` and `ip netns exec` using `nix::sched::setns`
- Expand rtnetlink for link state management
- ~70% reduction in process spawning

**Remaining dependency:**
- `tc` binary for netem operations
- No viable Rust alternative exists as of late 2025
- This is acceptable given the low frequency of TC operations

**Recommended next steps:**
1. Add `nix = { version = "0.30", features = ["sched"] }` to dependencies
2. Implement `run_in_namespace()` helper using setns
3. Migrate container and namespace operations incrementally
4. Monitor rust-tc/netlink-tc for write support developments

---

## References

- [rust-netlink organization](https://github.com/rust-netlink)
- [netlink-tc crate](https://crates.io/crates/netlink-tc)
- [netlink-packet-route docs](https://docs.rs/netlink-packet-route/latest/netlink_packet_route/)
- [nix::sched::setns](https://docs.rs/nix/latest/nix/sched/fn.setns.html)
- [netns-rs](https://github.com/openanolis/netns-rs)
- [libnl netem implementation](https://www.infradead.org/~tgr/libnl/doc/api/group__qdisc__netem.html) (C reference)
- [Linux tc netlink specification](https://docs.kernel.org/networking/netlink_spec/tc.html)
