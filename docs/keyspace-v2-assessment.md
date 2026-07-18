# Should tcgui adopt the ZenSight keyspace-v2 convention?

**An assessment of** [`p13marc/zensight/docs/rfcs/keyspace-v2`](https://github.com/p13marc/zensight/tree/master/docs/rfcs/keyspace-v2)
(13 chapters, status **v1.2 — RATIFIED**, last amended 2026-07-14)
**against tcgui at `e95983d`.**

*Revision 2 — 2026-07-14. Revision 1 assumed tcgui was a single-host tool, that generic
tooling was not a goal, and that backward compatibility constrained us. All three were wrong,
and all three cut the same way. This revision reverses two of revision 1's verdicts and answers
a question revision 1 did not ask.*

---

## 1. Verdict

**Adopt it — essentially in full, including the chapter I rejected last time. And yes: four RFC
amendments should land first, one of which is load-bearing.**

Three facts changed the analysis:

1. **tcgui is a fleet** — one frontend, N backends. Everything revision 1 called "latent" (the
   `@rpc` plane's hermeticity, `*`-origin fan-in, per-host ACL, the one-host drill-down
   selector) is now *the point*, not future-proofing.
2. **Following the RFC unlocks `zenctl`** — the bus explorer that RFC 08 §6 specifies into
   existence. That inverts revision 1's rejection of chapter 08: **reject the registry, reject
   zenctl.**
3. **We can break the wire.** The single largest cost in revision 1's accounting evaporates.

So the recommendation is stronger and simpler than last time. But the question *"do we need to
update the RFCs beforehand?"* deserved a real investigation rather than a reflexive "no", and the
honest answer is **yes** — I tried to falsify that and failed. §3 is the substance of this
revision.

---

## 2. What the RFC is

```
<base>/v1/<origin>/<class>/<producer>/<subject...>
```

| Position | Chunk | Meaning |
|---|---|---|
| 1 | **base** | deployment root; set as the Zenoh session `namespace`, so app code never spells it |
| 2 | **version** | plain `v<int>`; a `v1` selector can never match a `v2` key |
| 3 | **origin** | *who published it*: a stable opaque host id `h-<12hex>`, or a verbatim service origin (`@catalog`) |
| 4 | **class** | *bus semantics*: `telemetry` (superseded) · `state` (LWW + tombstones) · `events` (immutable) — or a **verbatim plane**: `@rpc`, `@media`, `@blob` |
| 5 | **producer** | the component that produced it (`name` or `name-<int>`) |
| 6+ | **subject** | the open-ended, registry-governed meaning path |

Three ideas carry it. **Verbatim chunks make hermetic planes** — a Zenoh wildcard never crosses
an `@`, so `tcgui/v1/h-xxx/**` delivers one host's entire data plane and *structurally cannot*
pull RPC traffic. **Class is a fixed position**, so storage, QoS, ACL and bandwidth policy become
static literal prefixes rather than application knowledge. **Identity is opaque and in the key**;
hostnames are payload, correctable without re-keying.

The document is credible because it has been **paid for**. Both amendments since ratification are
post-mortems, and both land on tcgui:

- **v1.1** — the version chunk was originally verbatim (`@v1`), which silently broke zenoh-ext's
  advanced pub/sub: it parks publisher-detection tokens at `<key>/@adv/pub/…` and parses them back
  with `${remaining:**}/@adv/…`, and `**` cannot cross an `@`. Late-publisher detection was dead;
  the only symptom was a log line. **tcgui uses `publisher_detection()` on every publisher and
  `detect_late_publishers()` on every subscriber** (`tcgui-backend/src/network.rs:119`,
  `bandwidth.rs:405`, `zenoh_query.rs:603`; `tcgui-frontend/src/zenoh_manager.rs:11`). We would
  have walked into this.
- **v1.2 (06 §6)** — the reference GUI keyed its device table on the payload *hostname* and built
  origin-scoped keys from it. Every drill-down in the product broke at once while telemetry kept
  streaming. Their own retrospective files the residue as issue #474: *"first thing to break in
  the container/multi-machine deployment"* — **which is literally tcgui's deployment.**

---

## 3. Do we need to update the RFC first? **Yes — four amendments.**

I set out to confirm that the RFC, being application-neutral by design, needed nothing before an
actuator could adopt it. I ran an adversarial pass specifically to *falsify* that claim. **It
falsified it.** Four genuine gaps and two hardenings, below. I verified the headline finding
against the chapter text myself.

The common root is simple and forgivable: **every producer in ZenSight is a sensor. It observes.
tcgui's backend mutates a shared kernel resource.** Nobody had written an actuator against this
convention yet. We would be the first, and these are the things the first one finds.

### G1 — the desired-state escape hatch is unspellable ★

The RFC's own sanctioned answer to durable commands does not compile. This is a **self-inflicted
contradiction introduced by v1.2**:

- **12 §3** decides durable commands are RPC-only, *"with (b) as the sanctioned escape hatch"* —
  (b) being *"a controller publishes `state/<producer>/desired/<topic>` … already expressible in
  the grammar with **zero new mechanism**."* **05 §3** repeats the key shape.
- That key is `<base>/v1/<origin>/state/<producer>/desired/<topic>`. `<origin>` must be the
  **target host** — but the **publisher is the controller**.
- **07 §3**, added *later*, in v1.2 (`07-bulk-planes.md:119-128`, verbatim):
  > **"A publisher MUST always use its concrete origin."** … *"A `*` in a published key is never a
  > shortcut; it is a lie about identity, and it is **unrepresentable if the origin is a value the
  > publisher owns rather than a string it formats**."*
- **08 §1.1**'s typed-origin table has exactly two cells: `local` (*"used to **serve**: publish
  state/telemetry"*) and `remote` (*"used to **call**"*). **"Publish under a remote origin" is not
  in the table.**
- **04 §2 R6**: *"the data planes are strictly producer→consumer."*
- **09 §3** denies it in ACL: the console gets `put`/`delete` **egress only**, and even `@catalog`
  is refused ingress-put, *"or it could tombstone ANY host's keys, defeating the per-host
  enrollment story."*

For tcgui this is not academic. *"These twelve hosts should hold 100 ms of delay, and reapply it
after reboot"* is the first thing anyone asks a **fleet** traffic-shaper for. Today's tcgui does
not need it — qdiscs survive a daemon restart because they are kernel state — but a reboot wipes
them, and a fleet makes declarative shaping the obvious next feature.

**The fix is grammar-legal and the RFC simply never states it:** a registered **service origin**
with the target host as the first subject chunk —

```
tcgui/v1/@tcdesired/state/h-3fa9c2d41b7e/default/eth0
```

— which preserves design property D4 (a verbatim origin stays out of every fleet selector), keeps
targeting in a *key position* rather than resurrecting the payload-target antipattern the RFC's
own P3 was written against, and costs one ACL rule. It needs an explicit carve-out from 07 §3 and
04 R6.

### G2 — nothing distinguishes a fan-out *read* from a fan-out *write*

**05 §2** presents `*`-origin fan-in as a first-class idiom. **07 §3** gates wildcards hard — but
only on `@media`/`@blob`, where the cost is *"measured in megabits."*

For tcgui the cost of a stray `*` is **40% packet loss applied to the entire fleet.** The RFC has
a normative rule where the cost is bandwidth and *none* where the cost is fleet-wide mutation.

This is not hypothetical. **`zenctl service call` takes the origin as a positional argument and
builds the key by string format — no registry lookup, no read/write distinction**
(`zenctl/src/main.rs:316-330`), and its own README demonstrates `zenctl service call '*' sysinfo
processes`. The tool we are adopting the RFC *in order to get* makes fleet-wide mutation a
one-character typo.

The hook already exists: **05 §3** distinguishes `read` / `write` / `long-running`, and **08 §2**
records `kind` in the registry. Nothing hangs off it. Amendment: a `fanout = forbidden | allowed`
field on procedures, plus a MUST in 05 §2.1. Additive, MINOR registry change, no wire impact.

### G3 — convergence is not a mutex, and side effects do not converge

**03 §1.5**'s anti-twin rule is a **SHOULD** (*"a producer SHOULD probe the liveliness key of its
intended producer chunk at startup and refuse to start over a live twin"*), and **04 §5** undercuts
it: *"Zenoh does **not** enforce token uniqueness … a token is presence, not a lock."*

The RFC's only real exclusivity protocol (**06 §5.3**, for `@catalog`) is unusually honest about
its limits: it accepts *"**eventual** single-writer, not linearizable single-writer"*, and its
load-bearing mitigation is **convergence** — the catalog is a pure function of live evidence, so
after a partition heals *"the surviving owner's next full recompute … overwrites interleaved
conclusions."*

**That property does not exist for an actuator.** A late unfenced write to a state *document* is
overwritten by the next recompute. A late unfenced `tc qdisc add` leaves 200 ms of delay on a
production NIC, and there is no recompute that removes it. Two tcgui daemons that both pass the
racy liveliness probe will both drive netlink.

Amendment: state that exclusivity for **side-effecting** producers is enforced *outside* the bus —
an OS-level lock (file lock, systemd unit, netlink exclusivity) — and that liveliness and claim
protocols are presence, never a mutex. Nothing currently stops a producer relying on 03 §1.5 for
this.

### G4 — the reference slug rule can emit an illegal chunk

**03 §2** requires non-verbatim chunks to match `[a-z0-9]([a-z0-9._-]*[a-z0-9])?` — *"must start
and end alphanumeric"* — and mandates a lossless `_xNN_` escape for anything outside the charset,
*"because plain `-` substitution is not injective."*

**Defect A — infinite regress.** `ip netns add _myns` is legal Linux, and tcgui's own
`validate_namespace` accepts it (`tcgui-shared/src/validation.rs:40`). `_myns` is not
charset-legal (leading `_`), so it must be escaped → `_x5f_myns` → **still starts with `_`** →
still illegal. The escape mechanism cannot produce a conforming chunk. Docker's name charset
happens to require an alphanumeric first character, which is why the observability side never hit
this; `ip netns` and `tc` class names do not.

**Defect B — case-sensitivity.** 03 §2 says *"Identifiers whose canonical text form is uppercase
MUST be lowercased at key-build time."* But **Linux interface names are case-sensitive** — `eth0`
and `ETH0` are two different NICs, and `validate_interface` explicitly permits uppercase
(`validation.rs:65`). Naive obedience collides them, violating the **injectivity MUST in the same
section**. On a close reading injectivity wins, so this is technically already covered — but for
an observability app the failure is a display glitch, whereas for tcgui it **shapes the wrong
NIC**. It warrants an editorial note at minimum.

Everything else in our charset is cleanly covered: `container:nginx` → `container_x3a_nginx`,
`veth1234@if5` → `veth1234_x40_if5`, and the Zenoh-illegal `*`/`$`/`?`/`#` fall out through the
same escape.

### G5, G6 — two hardenings

- **08 §1.1 typed origins are a SHOULD.** The outage they were written for (06 §6.3) was a *read*
  addressed at the caller's own host — *"The failure is a timeout, at runtime, in one view."* The
  same bug on a tcgui **write**, on a developer laptop that also runs a backend, **shapes the
  operator's own machine.** SHOULD → MUST for `kind = "write"` builders.
- **Sub-host ACL is inexpressible as written.** Zenoh ACL matches keyexpr *inclusion* and **cannot
  match selector parameters** (09 §3, fact 1). So `@rpc/tc/config/set?if=eth0` cannot express
  *"Alice may shape eth1 but not the management NIC."* The fix is grammar-legal today — lift the
  actuated resource into the procedure path (`@rpc/tc/config/{ns}/{if}/set`; interfaces are a
  bounded population, so 03 §2's per-message-data ban does not bite and 08 §2's `cardinality`
  field covers it). The RFC never says so because no observability RPC ever needed sub-host
  authority. This is guidance, not conflict — but we should take it into our key design (§7).

### What is *already* covered — and it is a lot

This matters as much as the gaps, so it gets equal billing. The RFC handles an actuator far
better than I expected, and **several tcgui features are its worked examples by name**:

- **Is an actuator even in scope?** Explicitly. **04 §2 R6**: *"Is it a question or an
  instruction? → `@rpc`."* And **10 §9** rejects Homie's alternative for exactly our reasons:
  *"suffix-discriminated commands — `<property>/set` sits inside the data tree, so a device
  wildcard pulls commands and controller writes race device reflections."*
- **Targeting is never payload.** **05 §2**: *"the convention has **no `target` field in any
  envelope**."*
- **The applied-TC-config echo is `state`.** **04 §1.2** names *"configuration echoes"* as state;
  **04 §3** gives them the `transition` QoS profile; **04 §3.3** names *"config echoes"* as one of
  the two places the advanced tier earns its cost. Our feature is their worked example.
- **An audit record of an apply is `events`.** **04 §1.3** lists, verbatim: *"config applied."*
- **Scenarios are 05 §3's long-running pattern**, unchanged: *"RPC to initiate, state to
  observe"* plus a cancel procedure. Extra verbs (pause/resume/loop) are pure registry entries —
  **05 §5** already ships a multi-verb control surface (`stream/open`, `close`, `keyframe`).
- **The preset/scenario library is `state` + `@rpc` CRUD**, and **05 §5**'s parallax
  stream-catalogue row is our exact template *including a trap we would otherwise ship*: *"a
  closed stream keeps its doc with `open: false` — tombstone on removal from config, not on
  close, or the UI loses the catalogue."*
- **Per-host shaping authority is 09 §3's `no-remote-actions` rule**, verbatim: *"dangerous
  procedures deniable per-key, because the key IS the target."*
- **N daemons on one host is 03 §1.5**'s instance suffix (`tc`, `tc-2`).
- **P11's chunk-order argument** is stated for an observability fleet — re-run it for a control
  plane and it comes out the same way, harder: per-host actuation, per-host ACL, per-host
  targeting.

**Four narrow amendments; everything else transfers unchanged.** That is a strong result, and it
is the argument for adopting rather than inventing.

---

## 4. What `zenctl` actually buys us

`zenctl` is *"a bus explorer for the keyspace-v2 convention — `busctl`/`d-feet`/`ros2` for a
fleet"*, and RFC 08 §6 *"specifies this tool into existence."* I read its source rather than its
README, because the difference matters.

**`--base` is already a flag** (`zenctl/src/main.rs:169`). The binding to ZenSight is in three
places: the compiled registry (`zensight_keyspace::registry::REGISTRIES`), the payload type table
(`zensight_common::PAYLOAD_TYPES` / `decode_payload`), and one `@catalog`-specific key.

| Command | Against a tcgui fleet, **unmodified** | Why |
|---|---|---|
| `zenctl node list --base tcgui` | ✅ **works today** | pure grammar — a liveliness query on `tcgui/v1/*/state/*/alive` |
| `zenctl service call h-3fa9 tc config/set --base tcgui --body @x.json` | ✅ **works today** | key built by string format; fan-in discipline (target `All`, consolidation `None`) built in |
| `zenctl topic echo 'tcgui/v1/**' --raw --base tcgui` | ⚠️ keys yes, payloads as hex | decoding needs the type table |
| `zenctl topic list` / `topic info` / `interface show` | ❌ | offline half reads ZenSight's compiled registry |
| `zenctl doctor` | ❌ | diffs fleet `introspect` against **ZenSight's** registry |

**About 40% of zenctl works on day one with `--base tcgui`** — including the two commands that
matter most operationally (who is up; call a procedure on one host, safely). The rest needs the
registry and type table made pluggable. **That is an upstream change in the zensight repo, not an
RFC change.**

**And there is a real RFC finding here.** 08 §6 promises that *"generic explorer tooling — the
`busctl`/`d-feet` equivalent — **needs no compiled-in registry**"*, and `introspect` already
returns each producer's registry slice **as TOML**. zenctl never uses it that way; it compiles one
in. The RFC's own type table (08 §5) already permits *"a schema URL"* as an alternative to a
crate-local item. **The convention is more generic than the tool it specified.** Closing that gap
is what makes keyspace-v2 genuinely multi-application — which is 01 §4's last stated goal — and
tcgui is the forcing function that would close it.

---

## 5. Adopt / reject / defer — revised

| Chapter | Rev. 1 | **Rev. 2** | Why it changed |
|---|---|---|---|
| **02** principles | Adopt | **Adopt** | — |
| **03** grammar | Adopt | **Adopt** | — |
| **03 §2** lexical rules + slugging | Adopt (bug fix) | **Adopt — with G4 erratum** | our netns/ifname charset breaks the reference escape |
| **04 §1–2** classes + placement | Adopt | **Adopt** | — |
| **04 §3.2/3.3** delivery tiers | Advice | **Advice** | — |
| **05** `@rpc` plane | "shape only; value is latent" | **Adopt — this is now the point** | a fleet makes D2/D4, fan-in and per-host ACL load-bearing |
| **06 §1** host origin | Adopt | **Adopt** | — |
| **06 §6** identity bridge | Mandatory | **Mandatory** | their #474 is *our* deployment |
| **08** registry + `introspect` | ❌ **Reject** | ✅ **ADOPT — reversed** | **it is the price of zenctl** |
| **06 §5** `@catalog` etc. | Reject | **Reject** | no correlation problem: no proxy producers, no evidence fusion |
| **07** `@media`/`@blob` | Reject | **Reject** | no frames, no bulk bytes |
| **09 §3** ACL | Defer | **Adopt when the fleet lands** | a tool that can `netem` a production host needs `no-remote-actions` |
| **09 §0.1** scouting | Read it | **Read it** | multicast and gossip are independent switches |
| **09 §6** cutover acceptance | Adopt if migrating | **Adopt** | — |
| **09 §2** storages/replication | Defer | **Defer** | revisit if we want a seed store |

The two reversals are the story: **chapter 08 flips from reject to adopt**, and the `@rpc` plane's
properties flip **from latent to load-bearing**. Revision 1's caveats ("`@rpc` hermeticity protects
nothing today"; "origin-first is fine, not load-bearing") were correct for a single-host tool and
are simply void for a fleet.

`@catalog` stays rejected, and I want to be clear it is not squeamishness: the catalog exists to
*fuse identity evidence from proxy producers into merged entities*. tcgui has no proxy producers
and observes no third-party devices. Origins are sufficient; there is nothing to correlate.

---

## 6. Findings against tcgui's code

All eight from revision 1 stand. Two are defects; the rest are debt the RFC names precisely. One
has been **upgraded** by the fleet fact.

### 6.1 Netlink names reach key expressions unvalidated, behind `.expect()` — **defect**

`topics::bandwidth_updates()` (`tcgui-shared/src/lib.rs:99`) is called from `bandwidth.rs:391`
with an interface name that came from *netlink discovery*, not a validated request. Same for
`tc_config` (`zenoh_query.rs:588`), `tc_statistics` (`main.rs:726`), `scenario_execution_updates`
(`scenario/execution.rs:766`). All end in `.expect()`.

`validate_interface` exists and is correct — but is only applied to *inbound query payloads*
(`zenoh_query.rs:77`, `:381`, `:490`). Discovered names never see it.

Linux's `dev_valid_name()` rejects only `/`, `:`, whitespace, `.` and `..`. So `*`, `**`, `?`, `#`,
`$` are all legal interface names. Zenoh key expressions forbid `#$?` — **but not `*`**
(`zenoh-keyexpr-1.9.0/src/key_expr/borrowed.rs:38`). `keformat`'s `set()` validates by
`keyexpr::new(value)` then `pattern.includes(ke)` (`format/mod.rs:531-536`), against a `*` pattern:

| Interface named | Result |
|---|---|
| `?`, `#`, `$…`, `**` | `FormatSetError` → **`.expect()` panics the privileged daemon** |
| `*` | `keyexpr::new("*")` succeeds, `*` includes `*` → **publishes on a wildcard key** |

The second is nastier: the backend declares an `AdvancedPublisher` on a *wildcard* key expression.
Reproduce with `ip link add name '*' type dummy`.

Fix: RFC 03 §2's slugging rule — with the G4 erratum applied.

### 6.2 `tcgui/storage/{backend}/scenarios/{id}` is off-grammar and dead — **defect**

`scenario/manager.rs:69`, `storage.rs:51/68/108/217`. Note where `storage` sits: **in the
`{backend}` position** — a side channel *beside* the identity chunk. That is exactly the Sparkplug
mistake RFC 03 §1.2 cites (STATE outside `spBv1.0/`, needed a breaking release to fix). It is also
dead: nothing configures a Zenoh storage plugin, so the puts go nowhere and the gets return zero
replies. The scenario `id` is interpolated with no validation.

### 6.3 Identity is a mutable, colliding, operator-chosen name

`backend_name` defaults to `"default"` (`config/cli.rs:246`) and is the discriminator in every key.
Two backends keeping the default publish to the same keys and the frontend cannot tell them apart.
RFC pain point **P2/P8**, verbatim. **In a fleet this stops being theoretical.**

### 6.4 We wrote half of chapter 08 and never used it

`extract_backend_name` and `extract_bandwidth_target` (`lib.rs:203`, `:214`) parse keys by
`split('/')` with magic indices — in a file that *already declares* `kedefine!` formatters whose
whole purpose is the build+parse pair RFC 08 §1 asks for. The frontend then bypasses `topics::`
entirely and hardcodes nine literals (`zenoh_manager.rs:374-544`). There is also a redundant second
`kedefine!` block (`lib.rs:76-84`) duplicating the first, whose only referenced item
(`backend_bandwidth_pattern`, `:110`) has no callers.

### 6.5 No class chunk, so `interfaces/list` and `interfaces/events` are two topics for one truth

Under RFC 04's rule **R1** both collapse into per-interface LWW state keys, where removal is a
`Delete` tombstone. The list/events reconciliation in `backend_manager.rs`/`app.rs` disappears.
Likewise `tc/{ns}/{if}` publishes `None` to mean "no TC config" (`main.rs:1052`,
`zenoh_query.rs:276/319`) — **04 §1.2** is explicit that retirement is a `SampleKind::Delete`,
*"never a payload marker."*

### 6.6 Failures ride the success payload instead of `reply_err`

`TcResponse` carries `success: bool` and `error_code` (`lib.rs:1018-1027`); `zenoh_query.rs` never
calls `reply_err` (`:37`, `:348`, `:395`, `:455`, `:521`). **05 §3** takes the D-Bus guideline
verbatim: *"a value reply always means success; a failure always rides Zenoh's reply-error
channel."* Note this is also a **zenctl** requirement — its README: *"An error reply goes to stderr
with its `error/...` name."*

### 6.7 The reply key is echoed from the query — **upgraded to a live bug by the fleet**

Every handler replies on `query.key_expr()`. Revision 1 called this latent. **With a fleet it is
not.** The moment anyone issues `tcgui/v1/*/@rpc/tc/diagnostics` — and `zenctl service call '*' …`
does exactly that — Zenoh's default consolidation keeps **one reply per reply key**, so a fleet all
replying on the shared wildcard key **collapses to a single survivor**. RFC 05 §2.1 makes this a
bolded MUST, and its own editorial note records that the reference implementation shipped it wrong
anyway. We satisfy the sibling rule (`@rpc` queryables must not be `complete`) only by accident —
no `.complete(` call exists in the backend.

### 6.8 The health key doubles as the liveliness token key

`main.rs:91-98` declares a liveliness token on `tcgui/{backend}/health`, the same key the health
document is published on (`zenoh_query.rs:549`). **04 §5** reserves a distinct `alive` leaf so
presence and data selectors cannot be confused — and `zenctl node list` reads exactly that leaf.

### 6.9 (Context) We run the fully-optioned advanced tier on every key

Every publisher: `cache(1)` + `sample_miss_detection(heartbeat(500ms))` + `publisher_detection()`.
Every subscriber: `history(detect_late_publishers())` + `recovery(heartbeat())` on **wildcard**
selectors. **04 §3.3** prices this at 4 entities per key, two of them unaggregatable routed
declarations, plus an unconditional heartbeat forever — and says *"prefer `periodic_queries(p)`
over `recovery(heartbeat)` on wide wildcard subscriptions."* Affordable at tens of keys; worth
revisiting at fleet × interface count.

---

## 7. The proposed `tcgui/v1/…` keyspace — fleet edition

`base` = `tcgui`, set as the session **namespace** from day one (their #466 was deferred and turned
out to be a *conformance gap*, not a preference). Producer chunk = `tc`.

### Origin: the host, not the container

```
origin = "h-" ++ lowercase_hex(sha256(machine_id_hex ++ "tcgui-host-id-v1"))[0..12]
```

The RFC is genuinely ambiguous here (06 §1 says *"all publishers on one machine derive the same
value"*; 06 §1.1 says containers *"MUST still mint a stable id"*), and for tcgui the choice is
forced: **host-as-origin.** Container-as-origin would give N origins mutating **one kernel's**
qdiscs, and *"host X may act only as itself"* would stop meaning anything about the resource.
Multiple backends on one host are **producer instances** (`tc`, `tc-2`), and the instance→netns
binding lives in the registration document (`state/tc/sensor`), because an ordinal carries no
meaning and must not be a chunk (P1/P8).

`backend_name` becomes a **display label** in the health document — never a key.

### Keys

```
# state — latest value is the truth; delete is a tombstone
tcgui/v1/{origin}/state/tc/health                        health doc + { host_id, name }
tcgui/v1/{origin}/state/tc/alive                         liveliness token (reserved leaf)
tcgui/v1/{origin}/state/tc/sensor                        registration doc (version, netns binding)
tcgui/v1/{origin}/state/tc/interface/{ns}/{if}           per-interface record   ← list + events
tcgui/v1/{origin}/state/tc/config/{ns}/{if}              applied TC config echo (QoS: transition)
tcgui/v1/{origin}/state/tc/execution/{ns}/{if}           scenario execution status
tcgui/v1/{origin}/state/tc/scenario/{scenario_id}        scenario library entry
tcgui/v1/{origin}/state/tc/preset/{preset_id}            preset library entry

# telemetry — superseded samples
tcgui/v1/{origin}/telemetry/tc/bandwidth/{ns}/{if}
tcgui/v1/{origin}/telemetry/tc/qdisc/{ns}/{if}

# events — immutable, rate-budgeted
tcgui/v1/{origin}/events/tc/applied/{ulid}               audit record of a TC apply

# @rpc — verbatim plane; failures ride reply_err
tcgui/v1/{origin}/@rpc/tc/config/{ns}/{if}/set           apply / clear TC        (write)
tcgui/v1/{origin}/@rpc/tc/interface/{ns}/{if}/set        enable / disable        (write)
tcgui/v1/{origin}/@rpc/tc/scenario/set                   scenario CRUD           (write)
tcgui/v1/{origin}/@rpc/tc/execution/{ns}/{if}/set        start/stop/pause/resume (write)
tcgui/v1/{origin}/@rpc/tc/diagnostics                    diagnostics             (read)
tcgui/v1/{origin}/@rpc/tc/introspect                     registry slice          (read) ← zenctl
```

**Note the resource is in the procedure path**, not a selector parameter (G5). That is what makes
*"Alice may shape eth1 but not the management NIC"* an ACL rule instead of an impossibility.

### Five traps the RFC saved us from, written down so we don't re-derive them

1. **Execution status is keyed by the interface, never by a run-id.** A `state/tc/execution/{ulid}`
   key would be per-message data in a published key — **03 §2** forbids it, with only four
   sanctioned exceptions, none of which is a state run-id.
2. **A scenario stepping every 500 ms must not emit per-step `events`.** **04 §1.3**'s rate budget
   is `rare` ≤ 1/h, `low` ≤ 1/min, and *"per-record streams that can burst unboundedly MUST NOT be
   events."* Current step goes in the LWW status doc; only the apply gets an audit record.
3. **A disabled interface keeps its doc with `up: false`.** Tombstone only when the NIC is *gone* —
   05 §5's parallax lesson verbatim, or the UI loses its catalogue of shapeable interfaces.
4. **Deleting a preset is a `SampleKind::Delete`,** not a payload marker (04 §1.2).
5. **Register the telemetry surface properly — no `{metric...}` catch-all.** See §8.

### Frontend selectors

```
tcgui/v1/*/state/**                  fleet state          (replaces 6 hardcoded literals)
tcgui/v1/*/telemetry/**              fleet telemetry
tcgui/v1/*/state/*/alive             liveliness — the whole presence protocol, zero payload
tcgui/v1/{origin}/**                 one host's complete data plane (drill-down)
```

Writes are **origin-scoped and concrete, always.**

### The identity bridge (06 §6) — not optional

```
BackendHealthStatus {
    host_id: "h-3fa9c2d41b7e",   // ← the origin. THE SAME VALUE. Build every key from this.
    name:    "lab-router",       // ← a display label. Never goes in a key.
}
```

The frontend **displays** `name` and **builds keys** from `host_id`. It **MUST NOT** key its
backend table on `name`: two backends sharing a name is not a cosmetic muddle — it is a table that
*builds keys*, so a collision **misroutes a shaping command to the wrong machine.** Enforce with a
`LocalOrigin`/`RemoteOrigin` type split (08 §1.1, hardened to MUST by G6).

---

## 8. What ZenSight's retrospective tells us not to repeat

`docs/plans/keyspace-v2/RETROSPECTIVE.md` is a gift. It also **prices the migration**: 20 commits,
289 files, **+9,987 / −5,519 lines**, one wire break, one production outage — for an app with 10
sensors, a GUI, a correlator and exporters. tcgui is 3 crates and 13 key families; scale
accordingly, but do not expect it to be small.

Four mistakes, each already paid for once:

1. **Do not register telemetry as a catch-all `{metric...}`** (their #468). Six producers did, so
   *"the lint is **vacuous** for telemetry — anything is buildable — and `introspect` tells a
   consumer nothing about which metrics a producer emits."* **That is precisely what zenctl needs.**
   tcgui's telemetry surface is two families and fully enumerable — register it properly on day one
   and `zenctl doctor` works for free.
2. **Do not key the frontend's table on the hostname** (their #474 — *"first thing to break in the
   container/multi-machine deployment"*). This is the outage, and their filed-but-unfixed issue is
   our day-one deployment.
3. **Set the session namespace immediately** (their #466) — deferring it was a conformance gap.
4. **Wire `introspect` to a consumer.** Their #469: *"We built the capability the RFC asks for and
   never used it."* For us the consumer is zenctl, so it is free — **but only if we do #1.**

Their §6.5 is the sentence to keep: *"That is the difference between paying for a convention and
benefiting from one."*

---

## 9. Recommendation

**Regardless of everything else — no wire change, do these now:**

1. Slug or reject netlink-supplied names before they enter a key (§6.1). Panic + wildcard-publish
   in a `CAP_NET_ADMIN` daemon.
2. Delete the dead `tcgui/storage/**` code (§6.2) and the duplicate `kedefine!` block (§6.4).
3. Reply on the queryable's own concrete key (§6.7) — a **live** bug once there is a fleet.
4. Move failures to `reply_err` with namespaced error names (§6.6).

**Then, in order:**

5. **Propose the four amendments upstream (G1–G4, plus G5/G6 as hardenings).** They are additive,
   none changes a byte on the wire, and we are the first actuator to adopt this convention — which
   is exactly the position from which they are discoverable. G1 in particular is a contradiction the
   RFC introduced against itself in v1.2; it should be fixed whether or not tcgui adopts.
6. **Cut over to `tcgui/v1/…`** (§7), with the registry (ch. 08) — because it is the price of
   zenctl, and because it is worthless if we take the catch-all shortcut.
7. **Get `zenctl` app-agnostic.** `node list` and `service call` work today with `--base tcgui`.
   The rest needs the registry and type table made pluggable upstream — and the honest version of
   that change is to make zenctl source the registry **from the bus** via `introspect`, which is
   what 08 §6 promised in the first place.

Skip `@catalog`, `@media`, `@blob`. Defer the storage cookbook. Take the ACL chapter when the fleet
lands — a tool that can apply `netem` to a production host wants `no-remote-actions` more than a
telemetry sensor ever did.

---

## 10. How we would know it worked

RFC 09 §6 sets the bar, and a fleet actuator adds one more:

1. **The retired key family is provably silent.** Subscribe to the whole old root and assert an
   empty result set while the v1 planes carry traffic. Because our version chunk is plain, the check
   must state its meaning explicitly (*anything outside `tcgui/v1/`*) rather than riding on key
   algebra.

2. **A consumer-shaped, concrete-key probe passes.**
   > *A test that uses a wildcard where the product uses a concrete value is testing a different
   > program.*

   A `tcgui/v1/*/@rpc/…` probe **cannot** catch a broken origin path — the `*` matches any origin,
   so a caller whose origin concept is garbage still gets replies. ZenSight's smoke was green while
   every drill-down in the product was dead. Our probe must resolve an origin through the same
   bridge the GUI uses, then issue origin-scoped calls, and **fail if the bridge yields nothing.**

3. **A fleet-write safety test** (ours, from G2). Assert that a `*`-origin call to a `kind = write`
   procedure is **refused** — at the builder, at the registry, or at the ACL. A traffic shaper that
   can be told to degrade the entire fleet by a one-character typo is not finished.

Run all three with multicast **off** and gossip **on** (09 §0.1) and an explicit endpoint — that is
isolation *and* a connected graph. Disabling "scouting" wholesale is not isolation; it is a silently
disconnected mesh.
