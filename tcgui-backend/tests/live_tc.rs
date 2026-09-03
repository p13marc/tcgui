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
