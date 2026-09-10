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
use tcgui_shared::{BackendHealthStatus, BackendMetadata};

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

/// (1) The retired key family is provably silent.
///
/// Because the version chunk is plain `v1`, the check states its meaning
/// explicitly — anything OUTSIDE `tcgui/v1/` — rather than riding on key
/// algebra.
///
/// Two things make this more than a re-assertion of a couple of literals:
///
/// - A **positive control** runs first: one deliberately pre-cutover key is
///   published and the leak buffer must catch it. Without that, a subscriber
///   that silently failed to declare would make the whole test pass.
/// - The v1 traffic enumerates the **entire generated key surface** — every
///   subject family, every procedure, the liveliness leaf and the blob prefix —
///   so a future registry entry or builder that emitted an off-root key would
///   fail here. `every_family_is_covered` keeps that enumeration honest.
///
/// Ceiling, stated so it is not overclaimed: this proves the *builders* never
/// leave `tcgui/v1/`. Proving the running daemon is silent on the old root
/// needs a spawned `tcgui-backend`, which needs CAP_NET_ADMIN and
/// /var/run/netns, so it cannot live in `cargo test`.
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

    // Positive control: a genuinely pre-cutover key MUST be caught.
    backend
        .put("tcgui/lab-router/interfaces/list", b"{}".to_vec())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    {
        let mut caught = leaked.lock().unwrap();
        assert_eq!(
            caught.len(),
            1,
            "the observer did not see a deliberately leaked old-root key — \
             this test cannot detect anything: {caught:?}"
        );
        caught.clear();
    }

    // Now the real thing: every key this producer can build.
    let o = mint_local_origin();
    for key in every_generated_key(&o) {
        backend.put(key.as_str(), b"{}".to_vec()).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let leaked = leaked.lock().unwrap();
    assert!(
        leaked.is_empty(),
        "traffic leaked outside tcgui/v1/: {leaked:?}"
    );
}

/// Every key the generated registry can build for one origin: one subject per
/// family, every `@rpc` procedure, the liveliness leaf and the blob prefix.
fn every_generated_key(o: &tcgui_shared::identity::LocalOrigin) -> Vec<String> {
    let mut keys: Vec<String> = vec![
        tc::key(o, &tc::Subject::Health).to_string(),
        tc::key(o, &tc::Subject::Sensor).to_string(),
        tc::key(o, &tc::Subject::interface("default", "eth0")).to_string(),
        tc::key(o, &tc::Subject::config("default", "eth0")).to_string(),
        tc::key(o, &tc::Subject::execution("default", "eth0")).to_string(),
        tc::key(o, &tc::Subject::scenario("demo")).to_string(),
        tc::key(o, &tc::Subject::preset("demo")).to_string(),
        tc::key(o, &tc::Subject::bandwidth("default", "eth0")).to_string(),
        tc::key(o, &tc::Subject::qdisc("default", "eth0")).to_string(),
        tc::key(o, &tc::Subject::plug("default", "eth0")).to_string(),
        tc::key(o, &tc::Subject::applied("01jabcdefghijkmnpqrstvwxyz")).to_string(),
        tcgui_shared::topics::state_alive(o).to_string(),
    ];
    for p in tc::ProcedureId::ALL {
        keys.push(tc::rpc_serve_key(o, *p).to_string());
    }
    keys.push(format!(
        "{}/manifest",
        zenkey::V1Context::with_producer(o.to_origin(), tc::producer())
            .blob_prefix(zenkey::grammar::BlobTier::Artifact)
    ));
    keys
}

/// The enumeration above must cover every registered family. If someone adds a
/// subject to `registry/tc.toml`, this fails and the silence test above is
/// extended rather than silently leaving the new family unchecked.
#[test]
fn every_family_is_covered() {
    let o = mint_local_origin();
    assert_eq!(
        tc::Family::ALL.len(),
        11,
        "a subject family was added or removed — extend every_generated_key()"
    );
    assert_eq!(
        tc::ProcedureId::ALL.len(),
        8,
        "a procedure was added or removed — extend every_generated_key()"
    );
    // 11 families + 1 alive leaf + 8 procedures + 1 blob prefix
    assert_eq!(every_generated_key(&o).len(), 21);
}

/// (2) A consumer-shaped, concrete-key probe that genuinely uses the identity
/// bridge.
///
/// A `tcgui/v1/*/@rpc/…` probe cannot catch a broken origin path — the `*`
/// matches any origin, so a caller whose origin concept is garbage still gets
/// replies. So this resolves the origin the way the GUI does: subscribe to
/// `topics::sel_state()` (the *exact* selector the GUI uses), receive a real
/// `BackendHealthStatus`, take `host_id`, and only then issue an
/// origin-scoped, concrete-key call.
///
/// Every step MUST-fails if the bridge is broken: no health sample → the
/// timeout fires; an unparseable key → `parse_state_key` is `None`; an empty or
/// junk `host_id` → `RemoteOrigin::parse` errors.
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
        .complete(false)
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

    // The GUI's own state subscription — not a hand-written key.
    let health_sub = frontend
        .declare_subscriber(tcgui_shared::topics::sel_state())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The backend publishes a real health document.
    let health = BackendHealthStatus {
        host_id: local.chunk().to_string(),
        backend_name: "display-label-only".to_string(),
        status: "running".to_string(),
        timestamp: 0,
        metadata: BackendMetadata::default(),
        namespace_count: 0,
        interface_count: 0,
    };
    backend
        .put(
            tc::key(&local, &tc::Subject::Health).as_keyexpr(),
            serde_json::to_vec(&health).unwrap(),
        )
        .await
        .unwrap();

    // A subscriber channel never closes, so a bare recv would hang CI forever
    // if the bridge were broken. Time it out and fail instead.
    let sample = tokio::time::timeout(Duration::from_secs(2), health_sub.recv_async())
        .await
        .expect("no health document arrived — the identity bridge yielded nothing")
        .expect("state subscription closed");

    let parsed = tcgui_shared::topics::parse_state_key(sample.key_expr().as_str())
        .expect("health key did not parse through the registry");
    assert_eq!(parsed.subject, tc::Subject::Health);

    let doc: BackendHealthStatus =
        serde_json::from_slice(sample.payload().to_bytes().as_ref()).expect("health doc decodes");

    // The assertion that earns its keep: the GUI backfills host_id from the key
    // when it is empty, which would paper over exactly this divergence.
    assert_eq!(
        doc.host_id, parsed.origin,
        "payload host_id diverged from the key origin — the bridge would misroute"
    );

    let remote = RemoteOrigin::parse(&doc.host_id).expect("bridge yielded no usable origin");

    // Origin-scoped, concrete key — built from what came off the wire.
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
