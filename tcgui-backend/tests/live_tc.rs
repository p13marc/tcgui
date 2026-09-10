//! Live tc/netlink tests against a real kernel (issue #21).
//!
//! Every other test in this directory is mock-based (`MockTcCommandManager`),
//! which is what lets them run anywhere. These do the opposite: they create a
//! throwaway network namespace, put a dummy interface in it, and drive
//! [`TcCommandManager`] against the actual kernel. Some of what this module
//! covers — above all the delete+add strategy for parameter removal — cannot be
//! verified by a mock at all, because the thing being tested is how the *kernel*
//! responds.
//!
//! # Running them
//!
//! ```text
//! just test-live
//! ```
//!
//! # Why both `#[ignore]` and `require_root!()`
//!
//! `#[ignore]` is the hard gate: `cargo test --workspace --all-targets
//! --all-features` — what `just ci` runs — compiles these but never runs them,
//! whatever user CI happens to execute as. That matters because self-hosted
//! runners not uncommonly run as root, and creating network namespaces on a
//! shared runner is not something to leave to chance.
//!
//! `require_root!()` is the courtesy gate: a developer who runs
//! `cargo test -- --ignored` without root gets a clean skip instead of a pile
//! of permission errors.
//!
//! # Cleanup
//!
//! `LabNamespace::drop` deletes the namespace and, if that fails, logs the exact
//! `ip netns del` command to recover. The workspace uses the default
//! `panic = "unwind"`, so Drop still runs when an assertion fails. Namespaces
//! are named `nlink-lab-<prefix>-<pid>-<counter>`, so anything left behind by a
//! SIGKILL is identifiable. Each test uses its own namespace, so they do not
//! interact.

use nlink::lab::LabNamespace;
use tcgui_backend::tc_commands::TcCommandManager;
use tcgui_shared::{
    TcCorruptConfig, TcDelayConfig, TcDuplicateConfig, TcLossConfig, TcNetemConfig,
    TcRateLimitConfig, TcReorderConfig,
};

/// A config with every feature off; tests switch on only what they assert.
fn empty_config() -> TcNetemConfig {
    TcNetemConfig {
        loss: TcLossConfig {
            enabled: false,
            percentage: 0.0,
            correlation: 0.0,
        },
        delay: TcDelayConfig {
            enabled: false,
            base_ms: 0.0,
            jitter_ms: 0.0,
            correlation: 0.0,
        },
        duplicate: TcDuplicateConfig {
            enabled: false,
            percentage: 0.0,
            correlation: 0.0,
        },
        reorder: TcReorderConfig {
            enabled: false,
            percentage: 0.0,
            correlation: 0.0,
            gap: 0,
        },
        corrupt: TcCorruptConfig {
            enabled: false,
            percentage: 0.0,
            correlation: 0.0,
        },
        rate_limit: TcRateLimitConfig {
            enabled: false,
            ..Default::default()
        },
    }
}

fn loss(percentage: f32) -> TcLossConfig {
    TcLossConfig {
        enabled: true,
        percentage,
        correlation: 0.0,
    }
}

fn delay(base_ms: f32) -> TcDelayConfig {
    TcDelayConfig {
        enabled: true,
        base_ms,
        jitter_ms: 0.0,
        correlation: 0.0,
    }
}

/// Namespace + dummy interface, up and ready to be shaped.
fn lab(prefix: &str, iface: &str) -> nlink::Result<LabNamespace> {
    let ns = LabNamespace::new(prefix)?;
    ns.add_dummy(iface)?;
    ns.link_up(iface)?;
    Ok(ns)
}

/// **The test that justifies this whole module.**
///
/// Disabling one netem feature while keeping another must actually clear it in
/// the kernel. This is the behaviour CLAUDE.md describes as needing the
/// delete+add strategy, because `tc netem replace` preserves parameters that
/// are left out. Only a real kernel can confirm the parameter is gone; a mock
/// asserts our own bookkeeping back to us.
///
/// Note it asserts the *observable outcome*, not the mechanism. Verified by
/// mutation: forcing the `replace` branch (disabling
/// `requires_recreation_for`) leaves this test passing, because nlink sends a
/// complete `NetemConfig` rather than omitting attributes the way the `tc` CLI
/// does — so `replace` already writes delay=0. The delete+add path is
/// belt-and-braces for this case rather than load-bearing. Asserting the
/// outcome is still right: a test that pinned the mechanism would fail on a
/// refactor that kept the behaviour correct.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_disabled_parameter_is_cleared_in_the_kernel() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = lab("tcgui-removal", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(5.0);
    config.delay = delay(20.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply loss+delay");

    let opts = tc
        .get_netem_options(ns.name(), "dummy0")
        .await
        .expect("read back")
        .expect("netem present after apply");
    assert!(opts.loss().unwrap_or(0.0) > 0.0, "loss was not applied");
    assert!(
        opts.delay().map(|d| d.as_millis()).unwrap_or(0) > 0,
        "delay was not applied"
    );

    // Now drop delay, keeping loss. Under a plain `replace` the kernel would
    // keep the 20ms.
    config.delay = TcDelayConfig {
        enabled: false,
        base_ms: 0.0,
        jitter_ms: 0.0,
        correlation: 0.0,
    };
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("re-apply without delay");

    let opts = tc
        .get_netem_options(ns.name(), "dummy0")
        .await
        .expect("read back")
        .expect("netem still present");
    assert_eq!(
        opts.delay().map(|d| d.as_millis()).unwrap_or(0),
        0,
        "delay survived removal — disabling the feature did not reach the kernel"
    );
    assert!(
        opts.loss().unwrap_or(0.0) > 0.0,
        "loss was lost while removing delay"
    );
    Ok(())
}

/// Apply, read back, clear — the removal spine.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_apply_read_back_clear() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = lab("tcgui-spine", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(10.0);
    config.delay = delay(50.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply");

    let opts = tc
        .get_netem_options(ns.name(), "dummy0")
        .await
        .expect("read back")
        .expect("netem present");
    assert!(
        (opts.loss().unwrap_or(0.0) - 10.0).abs() < 1.0,
        "loss ≈ 10%"
    );
    assert!(
        (opts.delay().map(|d| d.as_millis()).unwrap_or(0) as i64 - 50).abs() <= 2,
        "delay ≈ 50ms"
    );

    tc.remove_tc_config_in_namespace(ns.name(), "dummy0")
        .await
        .expect("remove");
    assert!(
        tc.get_netem_options(ns.name(), "dummy0")
            .await
            .expect("read back after remove")
            .is_none(),
        "netem still present after remove"
    );
    Ok(())
}

/// capture -> change -> restore. This is the scenario engine's rollback path
/// (`execution.rs` cleanup-on-failure), and nothing exercised it against a
/// kernel before.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_capture_restore_roundtrip() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = lab("tcgui-restore", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut original = empty_config();
    original.loss = loss(7.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &original)
        .await
        .expect("apply original");

    let captured = tc
        .capture_tc_state(ns.name(), "dummy0")
        .await
        .expect("capture");
    assert!(captured.had_netem, "capture missed the netem qdisc");
    let captured_loss = captured
        .netem_config
        .as_ref()
        .expect("captured config")
        .loss
        .percentage;
    assert!((captured_loss - 7.0).abs() < 1.0, "captured loss ≈ 7%");

    // Something else runs, then we roll back.
    let mut interim = empty_config();
    interim.loss = loss(40.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &interim)
        .await
        .expect("apply interim");

    tc.restore_tc_state(&captured).await.expect("restore");
    let opts = tc
        .get_netem_options(ns.name(), "dummy0")
        .await
        .expect("read back")
        .expect("netem present after restore");
    assert!(
        (opts.loss().unwrap_or(0.0) - 7.0).abs() < 1.0,
        "restore did not bring back the original loss"
    );
    Ok(())
}

/// Removing from an interface that has no netem is a no-op, not an error —
/// `del_qdisc_if_exists` folds the "nothing there" case to `Ok(false)`.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_remove_on_clean_interface_is_ok() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = lab("tcgui-clean", "dummy0")?;
    let tc = TcCommandManager::new();

    tc.remove_tc_config_in_namespace(ns.name(), "dummy0")
        .await
        .expect("removing from a clean interface should succeed");
    Ok(())
}

/// Applying to an interface that does not exist fails cleanly, with the
/// interface name in the message rather than a bare kernel errno.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_apply_to_missing_interface_errors() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = lab("tcgui-missing", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(5.0);
    let err = tc
        .apply_tc_config_structured(ns.name(), "definitely-absent", &config)
        .await
        .expect_err("applying to a missing interface should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("definitely-absent") || msg.to_lowercase().contains("not found"),
        "error does not identify the missing interface: {msg}"
    );
    Ok(())
}

/// Queue statistics come back once a qdisc exists.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_tc_statistics_present_after_apply() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = lab("tcgui-stats", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.delay = delay(10.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply");

    let stats = tc
        .get_tc_statistics(ns.name(), "dummy0")
        .await
        .expect("statistics query");
    assert!(stats.is_some(), "no statistics for a shaped interface");
    Ok(())
}

// ---------------------------------------------------------------------------
// Plug qdisc (issue: nlink 0.26 adoption)
// ---------------------------------------------------------------------------
//
// These are the tests that justify the placement decision. The plug is grafted
// as netem's single leaf rather than replacing netem at the root, and only a
// real kernel can say whether that graft is legal, whether netem's parameters
// survive it, and whether `replace` leaves the child alone. A mock would just
// read our own bookkeeping back to us.
//
// Each is gated on `sch_plug` being available: it is a module on most distro
// kernels, and a machine without it should skip rather than fail.

/// Skip helper: `sch_plug` is a module, not always built in.
fn plug_available() -> bool {
    if nlink::lab::has_module("sch_plug") {
        return true;
    }
    eprintln!("skipping: sch_plug is not available on this kernel");
    false
}

/// **The test that justifies choosing a netem child over a netem replacement.**
///
/// `sch_plug` is classless, so it cannot host netem; netem is classful with
/// exactly one leaf, so it can host the plug. If that graft were illegal the
/// whole design would collapse back to "plug replaces netem and you lose every
/// impairment while stalled". Assert the tree *and* that netem's parameters
/// still read back.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_plug_installs_as_a_netem_child() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-graft", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(5.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply loss");

    let snap = tc
        .plug_begin(ns.name(), None, "dummy0", Some(64 * 1024))
        .await
        .expect("plug installs as a netem child");
    assert!(snap.buffering, "plug did not report a buffering epoch");
    assert!(
        !snap.netem_synthesized,
        "netem already existed; it must not be reported as synthesized"
    );

    let probed = tc
        .plug_probe(ns.name(), None, "dummy0")
        .await
        .expect("probe")
        .expect("plug is installed");
    assert_eq!(probed.0, snap.parent, "plug is not at the recorded parent");

    // The impairment must survive the graft — that is the whole point.
    let opts = tc
        .get_netem_options(ns.name(), "dummy0")
        .await
        .expect("read back")
        .expect("netem is still the root qdisc");
    assert!(
        opts.loss().unwrap_or(0.0) > 0.0,
        "loss was lost when the plug was grafted"
    );
    Ok(())
}

/// The way out of a plug, which is the property nlink 0.26 exists to provide:
/// releasing leaves the qdisc installed and stops it holding packets.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_release_indefinite_is_the_way_out() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-release", "dummy0")?;
    let tc = TcCommandManager::new();

    let snap = tc
        .plug_begin(ns.name(), None, "dummy0", None)
        .await
        .expect("plug");
    tc.plug_release(ns.name(), None, "dummy0", snap.parent)
        .await
        .expect("release must be reachable");

    assert!(
        tc.plug_probe(ns.name(), None, "dummy0")
            .await
            .expect("probe")
            .is_some(),
        "release removed the qdisc; it should only end the epoch"
    );
    Ok(())
}

/// A plug action against an interface with no plug must fail cleanly rather
/// than creating one. `change_qdisc` sends handle 0 without `NLM_F_CREATE`, so
/// the kernel answers ENOENT — pin that reading, because the whole control
/// surface depends on it.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_plug_action_without_a_plug_is_a_clean_error() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-absent", "dummy0")?;
    let tc = TcCommandManager::new();

    let err = tc
        .plug_release_one(ns.name(), None, "dummy0", nlink::TcHandle::new(1, 1))
        .await
        .expect_err("releasing a plug that does not exist must fail");
    // Must not have conjured one.
    assert!(
        tc.plug_probe(ns.name(), None, "dummy0")
            .await
            .expect("probe")
            .is_none(),
        "a failed release created a plug qdisc: {err}"
    );
    Ok(())
}

/// Non-obvious kernel behaviour the design leans on: applying an impairment
/// *change* goes down the `replace` path, and `replace` on the same kind takes
/// the kernel's `qdisc_change` route, which leaves the grafted child alone.
/// If this ever stops being true, plugs would vanish whenever someone moved a
/// slider.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_replace_preserves_the_plug_child() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-replace", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(5.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply loss");
    let snap = tc
        .plug_begin(ns.name(), None, "dummy0", None)
        .await
        .expect("plug");

    // Raising loss adds no parameter, so this is the replace branch.
    config.loss = loss(10.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("raising loss must be allowed while plugged");

    let probed = tc
        .plug_probe(ns.name(), None, "dummy0")
        .await
        .expect("probe")
        .expect("the plug survived a netem replace");
    assert_eq!(probed.0, snap.parent, "the plug moved");
    Ok(())
}

/// The other half: a parameter *removal* needs the qdisc recreated, which the
/// kernel implements as delete+add — and the child goes with the parent. That
/// must be refused with a message naming the plug, not silently swallowed.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_recreation_while_plugged_is_refused() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-recreate", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(5.0);
    config.delay = delay(20.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply loss+delay");
    tc.plug_begin(ns.name(), None, "dummy0", None)
        .await
        .expect("plug");

    // Dropping delay is the recreation branch.
    config.delay = TcDelayConfig {
        enabled: false,
        base_ms: 0.0,
        jitter_ms: 0.0,
        correlation: 0.0,
    };
    let err = tc
        .apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect_err("removing a parameter while plugged must be refused");
    assert!(
        err.to_string().contains("plug"),
        "the refusal must name the plug, got: {err}"
    );
    assert!(
        tc.plug_probe(ns.name(), None, "dummy0")
            .await
            .expect("probe")
            .is_some(),
        "the refused apply destroyed the plug anyway"
    );
    Ok(())
}

/// Removing the plug from an interface that had no qdisc must leave it as it
/// was found — the synthesized netem goes too, or the GUI would report a TC
/// config the operator never asked for.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_synthesized_netem_is_cleaned_up() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-synth", "dummy0")?;
    let tc = TcCommandManager::new();

    let snap = tc
        .plug_begin(ns.name(), None, "dummy0", None)
        .await
        .expect("plug on a bare interface");
    assert!(
        snap.netem_synthesized,
        "a netem was created to host the plug but not recorded as synthesized"
    );

    tc.plug_remove(
        ns.name(),
        None,
        "dummy0",
        snap.parent,
        snap.netem_synthesized,
    )
    .await
    .expect("remove");

    assert!(
        tc.plug_probe(ns.name(), None, "dummy0")
            .await
            .expect("probe")
            .is_none(),
        "the plug is still installed"
    );
    let qdisc = tc
        .check_existing_qdisc(ns.name(), "dummy0")
        .await
        .expect("read root qdisc");
    assert!(
        !qdisc.contains("netem"),
        "the synthesized netem was left behind: {qdisc}"
    );
    Ok(())
}

/// Clearing an interface's TC config releases the plug before tearing the root
/// down, so buffered packets are delivered rather than freed by
/// `qdisc_reset_queue`. Asserts the observable outcome: both qdiscs gone.
#[tokio::test]
#[ignore = "requires root; run with `just test-live`"]
async fn live_remove_tc_releases_before_deleting() -> nlink::Result<()> {
    nlink::require_root!();
    if !plug_available() {
        return Ok(());
    }
    let ns = lab("tcgui-plug-clear", "dummy0")?;
    let tc = TcCommandManager::new();

    let mut config = empty_config();
    config.loss = loss(5.0);
    tc.apply_tc_config_structured(ns.name(), "dummy0", &config)
        .await
        .expect("apply loss");
    tc.plug_begin(ns.name(), None, "dummy0", None)
        .await
        .expect("plug");

    tc.remove_tc_config_in_namespace(ns.name(), "dummy0")
        .await
        .expect("clear must release the plug and remove both qdiscs");

    assert!(
        tc.plug_probe(ns.name(), None, "dummy0")
            .await
            .expect("probe")
            .is_none(),
        "the plug outlived the root qdisc"
    );
    Ok(())
}
