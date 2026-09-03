//! A malformed request must produce a reply-**error**, not silence (#42).
//!
//! Before this, every request-decode failure — missing payload, oversize,
//! non-UTF-8, malformed JSON — bailed out of its handler with `?` into a caller
//! that only logged. The querier got no reply at all and simply timed out. A
//! *semantically* invalid request got a clean `error/tc/invalid-request`; a
//! *syntactically* invalid one got nothing, which is the worse outcome.
//!
//! This exercises the shared decode seam (`tcgui_shared::rpc::decode_request`)
//! plus the real Zenoh reply-error mechanics. It deliberately does not try to
//! stand up a `TcBackend`: that type holds a live session, a `NetworkManager`
//! and a `TcCommandManager`, its constructor opens netlink and reads
//! /etc/machine-id, and it lives in the binary crate rather than the library —
//! so a test cannot build one, and `just ci` runs unprivileged anyway. What is
//! left untested is the three-line `match` in each handler, which `cargo check`
//! covers structurally.

use std::time::Duration;

use tcgui_shared::TcRequest;
use tcgui_shared::rpc::{decode_request, reply_error_message};

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

/// A queryable wired exactly the way the real handlers are: decode through the
/// shared seam, and on failure reply on the error channel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_request_rides_the_reply_error_channel() {
    let ep = "tcp/127.0.0.1:17461";
    let backend = make_session(Some(ep), None).await;
    let client = make_session(None, Some(ep)).await;

    let key = "v1/h-000000000001/@rpc/tc/config/default/eth0/set";
    let _q = backend
        .declare_queryable(key)
        .complete(false)
        .callback(move |q| {
            tokio::spawn(async move {
                match decode_request::<TcRequest>(q.payload(), "tc") {
                    Ok(_) => {
                        let _ = q.reply(q.key_expr().clone(), b"{}".to_vec()).await;
                    }
                    Err(fault) => {
                        let _ = q.reply_err(fault.wire()).await;
                    }
                }
            });
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    for (label, payload) in [
        ("malformed JSON", b"{not json".to_vec()),
        ("wrong shape", br#"{"unexpected":1}"#.to_vec()),
        ("non-UTF-8", vec![0xff, 0xfe, 0xfd]),
    ] {
        let replies = client.get(key).payload(payload).await.unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(2), replies.recv_async())
            .await
            .unwrap_or_else(|_| panic!("{label}: no reply at all — the handler went silent"))
            .expect("reply channel closed");

        let err = reply
            .result()
            .expect_err(&format!("{label}: got a value reply instead of reply_err"));
        let message = reply_error_message(err);
        assert!(
            message.starts_with("error/tc/invalid-request: "),
            "{label}: error name missing or wrong: {message:?}"
        );
    }
}

/// The happy path still rides the value channel — otherwise the test above
/// would pass with a handler that simply errors on everything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_well_formed_request_still_gets_a_value_reply() {
    let ep = "tcp/127.0.0.1:17462";
    let backend = make_session(Some(ep), None).await;
    let client = make_session(None, Some(ep)).await;

    let key = "v1/h-000000000002/@rpc/tc/config/default/eth0/set";
    let _q = backend
        .declare_queryable(key)
        .complete(false)
        .callback(move |q| {
            tokio::spawn(async move {
                match decode_request::<TcRequest>(q.payload(), "tc") {
                    Ok(_) => {
                        let _ = q.reply(q.key_expr().clone(), b"{}".to_vec()).await;
                    }
                    Err(fault) => {
                        let _ = q.reply_err(fault.wire()).await;
                    }
                }
            });
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let request = TcRequest {
        namespace: "default".to_string(),
        interface: "eth0".to_string(),
        operation: tcgui_shared::TcOperation::Remove,
    };
    let replies = client
        .get(key)
        .payload(serde_json::to_vec(&request).unwrap())
        .await
        .unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(2), replies.recv_async())
        .await
        .expect("no reply to a well-formed request")
        .expect("reply channel closed");
    assert!(
        reply.result().is_ok(),
        "a well-formed request should ride the value channel"
    );
}
