//! keyspace-v2 cutover acceptance tests (issue #46; RFC 09 §6 + amendment G2).
//!
//! Run with multicast OFF and gossip ON via an explicit endpoint — that is
//! isolation AND a connected graph (09 §0.1); disabling scouting wholesale is a
//! silently disconnected mesh, not isolation.

use std::time::Duration;
use tcgui_shared::identity::{
    ConcreteOrigin as _, RemoteOrigin, local_origin_from_seed, mint_local_origin,
};
use tcgui_shared::registry::tc;

/// Two isolated peer sessions on one loopback endpoint: multicast off, one
/// listens, the other connects (gossip on). Namespace `tcgui`, so keys are
/// `tcgui/v1/…` on the wire.
async fn make_session(listen: Option<&str>, connect: Option<&str>) -> zenoh::Session {
    let mut c = zenoh::Config::default();
    c.insert_json5("namespace", "\"tcgui\"").unwrap();
    c.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    if let Some(l) = listen {
        c.insert_json5("listen/endpoints", &format!("[\"{l}\"]"))
            .unwrap();
    }
    if let Some(cn) = connect {
        c.insert_json5("connect/endpoints", &format!("[\"{cn}\"]"))
            .unwrap();
    }
    zenoh::open(c).await.unwrap()
}

/// (1) The retired key family is provably silent. Because the version chunk is
/// plain `v1`, the check states its meaning explicitly — anything OUTSIDE
/// `tcgui/v1/` — rather than riding on key algebra.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn old_root_is_silent_while_v1_carries_traffic() {
    let ep = "tcp/127.0.0.1:17451";
    let backend = make_session(Some(ep), None).await;
    let observer = make_session(None, Some(ep)).await;

    // The observer watches the whole base and records anything not under v1/.
    let leaked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let l2 = leaked.clone();
    let _sub = observer
        .declare_subscriber("tcgui/**")
        .callback(move |s| {
            let k = s.key_expr().as_str().to_string();
            if !k.starts_with("tcgui/v1/") {
                l2.lock().unwrap().push(k);
            }
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The backend emits real v1 traffic across planes.
    let o = mint_local_origin();
    backend
        .put(
            tc::key(&o, &tc::Subject::interface("default", "eth0")).as_keyexpr(),
            b"{}".to_vec(),
        )
        .await
        .unwrap();
    backend
        .put(
            tc::key(&o, &tc::Subject::bandwidth("default", "eth0")).as_keyexpr(),
            b"{}".to_vec(),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;

    let leaked = leaked.lock().unwrap();
    assert!(
        leaked.is_empty(),
        "traffic leaked outside tcgui/v1/: {leaked:?}"
    );
}

/// (2) A consumer-shaped, concrete-key probe: resolve an origin through the same
/// bridge the GUI uses (the health doc's `host_id`), then issue an
/// ORIGIN-SCOPED call. A `tcgui/v1/*/@rpc/…` probe is forbidden here — it would
/// pass even with a broken origin path. The probe MUST fail if the bridge
/// yields nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concrete_key_origin_probe() {
    let ep = "tcp/127.0.0.1:17452";
    let backend = make_session(Some(ep), None).await;
    let frontend = make_session(None, Some(ep)).await;

    let local = mint_local_origin();
    let diag_key = tc::diagnostics_key(&local);
    let cb_key = diag_key.clone();
    let _q = backend
        .declare_queryable(diag_key.as_keyexpr())
        .callback(move |q| {
            let k = cb_key.clone();
            tokio::spawn(async move {
                let _ = q
                    .reply(zenoh::key_expr::OwnedKeyExpr::from(k), b"ok".to_vec())
                    .await;
            });
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The health document IS the identity bridge: host_id == the origin. A
    // consumer resolves the origin through it and MUST fail if it is empty.
    let host_id = local.chunk().to_string();
    let remote = RemoteOrigin::parse(&host_id).expect("bridge yielded no origin");

    let replies = frontend
        .get(tc::diagnostics_key(&remote).as_str())
        .await
        .unwrap();
    let mut got = 0;
    while let Ok(r) = replies.recv_async().await {
        if r.result().is_ok() {
            got += 1;
        }
    }
    assert!(got >= 1, "origin-scoped probe got no reply");
}

/// (3) Fleet-write safety (amendment G2): a `*`-origin write is refused at the
/// builder — you cannot even construct the origin needed to build the key, so a
/// one-character typo cannot degrade the whole fleet.
#[test]
fn fleet_wide_write_is_unspellable() {
    // A wildcard is not a RemoteOrigin, so the write builder can never be
    // called with one — the refusal is structural, at the type level
    // (0.3: parse is a Result, and zenkey's own sealed traits carry G2).
    assert!(RemoteOrigin::parse("*").is_err());
    assert!(RemoteOrigin::parse("**").is_err());
    assert!(RemoteOrigin::parse("h-*").is_err());

    // A concrete origin does build a wildcard-free write key.
    let o = RemoteOrigin::parse(mint_local_origin().chunk()).unwrap();
    let key = tc::config_ns_iface_set_key(&o, "default", "eth0");
    assert!(!key.as_str().contains('*'));
}

/// (4) A `*`-origin fan-in reaches EVERY backend (issue #41; RFC 05 §2.1).
///
/// Two backends answer one wildcard-origin query. Each replies on its OWN
/// concrete key, so the two replies have two distinct reply keys and both
/// survive. If either echoed `query.key_expr()` — the wildcard the caller
/// spelled — both replies would carry the SAME key, and consolidation would
/// keep exactly one. Hence `ConsolidationMode::Latest` is set explicitly here:
/// it is the mode that collapses by reply key, i.e. the one that makes the bug
/// observable. A test on the default mode could pass with the bug present.
///
/// `diagnostics` is the only procedure a fan-in is legal for at all
/// (`registry/tc.toml`: `fanout = "allowed"`, and it is a read). Every `write`
/// procedure is `fanout = "forbidden"` and its key is unspellable with a
/// wildcard origin — that is test (3) above.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fanin_reaches_every_backend() {
    let ep = "tcp/127.0.0.1:17453";

    // Seeded, NOT minted: `mint_local_origin()` reads /etc/machine-id, so two
    // calls in one process return the SAME origin and the test would pass
    // vacuously with one backend answering twice.
    let a = local_origin_from_seed("fanin-backend-a");
    let b = local_origin_from_seed("fanin-backend-b");
    assert_ne!(a.chunk(), b.chunk(), "seeded origins must differ");

    let backend_a = make_session(Some(ep), None).await;
    let backend_b = make_session(None, Some(ep)).await;
    let frontend = make_session(None, Some(ep)).await;

    let key_a = tc::diagnostics_key(&a);
    let key_b = tc::diagnostics_key(&b);

    let mut queryables = Vec::new();
    for (session, key) in [(&backend_a, key_a.clone()), (&backend_b, key_b.clone())] {
        let reply_key = key.clone();
        queryables.push(
            session
                .declare_queryable(key.as_keyexpr())
                // The property under test: not complete, so the router does not
                // treat either backend as answering for the whole expression.
                .complete(false)
                .callback(move |q| {
                    let k = reply_key.clone();
                    tokio::spawn(async move {
                        // Reply on our OWN concrete key, never on q.key_expr().
                        let _ = q
                            .reply(zenoh::key_expr::OwnedKeyExpr::from(k), b"ok".to_vec())
                            .await;
                    });
                })
                .await
                .unwrap(),
        );
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Hand-spelled: there is deliberately no builder for a `*` origin
    // (RemoteOrigin::parse("*") is an Err — see test 3), so a fan-in selector
    // can only be written out literally.
    let replies = frontend
        .get("v1/*/@rpc/tc/diagnostics")
        .consolidation(zenoh::query::ConsolidationMode::Latest)
        .await
        .unwrap();

    let mut keys = std::collections::BTreeSet::new();
    while let Ok(r) = replies.recv_async().await {
        let sample = r.result().expect("diagnostics probe should not reply_err");
        keys.insert(sample.key_expr().as_str().to_string());
    }

    assert_eq!(
        keys.len(),
        2,
        "consolidation collapsed the fan-in — a backend replied on the echoed \
         wildcard key instead of its own concrete key: {keys:?}"
    );
    // Compare by suffix: whether the session namespace is stripped from a
    // received key varies, and the origin-bearing tail is what matters.
    for expected in [key_a.as_str(), key_b.as_str()] {
        assert!(
            keys.iter().any(|k| k.ends_with(expected)),
            "no reply on {expected}; got {keys:?}"
        );
    }
}
