//! The `@rpc` request/reply contract shared by both ends of the bus.
//!
//! RFC keyspace-v2 05 §3, taken verbatim from the D-Bus guideline: *a value
//! reply always means success; a failure always rides Zenoh's reply-error
//! channel*. This module owns the two halves of that contract that both crates
//! need — decoding an inbound request into a modelled fault, and rendering a
//! reply-error back into a string — so the backend and the GUI cannot drift on
//! the wording or the error names.

use serde::de::DeserializeOwned;

use crate::validation::MAX_REQUEST_PAYLOAD_BYTES;

/// A request that could not be decoded, in the shape it goes onto the
/// reply-error channel: a stable namespaced name plus the human detail.
///
/// The name is what tooling matches on (`zenctl` prints it to stderr); the
/// message is what a person reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFault {
    /// Stable `error/<service>/<kind>` slug.
    pub name: String,
    /// Human-readable detail.
    pub message: String,
}

impl QueryFault {
    fn invalid_request(service: &str, message: String) -> Self {
        Self {
            name: format!("error/{service}/invalid-request"),
            message,
        }
    }

    /// The exact bytes that go on the reply-error channel.
    pub fn wire(&self) -> String {
        format!("{}: {}", self.name, self.message)
    }
}

/// Decode a query payload into `T`, or produce the fault to put on the
/// reply-error channel.
///
/// `service` is the `error/<service>/…` prefix (`tc`, `interface`,
/// `diagnostics`). Before this existed, each of these failure shapes bailed out
/// of its handler with `?` into a caller that only logged, so a malformed
/// request produced **no reply at all** and the caller simply timed out — a
/// worse outcome than a semantically invalid one, which got a clean error.
pub fn decode_request<T: DeserializeOwned>(
    payload: Option<&zenoh::bytes::ZBytes>,
    service: &str,
) -> Result<T, QueryFault> {
    let payload = payload.ok_or_else(|| {
        QueryFault::invalid_request(service, format!("{service} request has no payload"))
    })?;

    let bytes = payload.to_bytes();
    if bytes.len() > MAX_REQUEST_PAYLOAD_BYTES {
        return Err(QueryFault::invalid_request(
            service,
            format!(
                "payload too large ({} bytes, limit {MAX_REQUEST_PAYLOAD_BYTES})",
                bytes.len()
            ),
        ));
    }

    let text = std::str::from_utf8(&bytes)
        .map_err(|e| QueryFault::invalid_request(service, format!("payload is not UTF-8: {e}")))?;

    serde_json::from_str::<T>(text).map_err(|e| {
        QueryFault::invalid_request(service, format!("payload is not valid JSON: {e}"))
    })
}

/// Render a Zenoh reply-error back into the `error/<service>: <detail>` string
/// the backend put on the wire.
///
/// Used by the GUI to surface a failure, and by the backend's own scenario
/// executor, which calls its TC procedure over the bus like any other client.
pub fn reply_error_message(err: &zenoh::query::ReplyError) -> String {
    let bytes = err.payload().to_bytes();
    match std::str::from_utf8(&bytes) {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => "backend reported an error".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Demo {
        _wanted: u32,
    }

    fn zbytes(s: &[u8]) -> zenoh::bytes::ZBytes {
        zenoh::bytes::ZBytes::from(s.to_vec())
    }

    #[test]
    fn missing_payload_is_a_modelled_fault() {
        let fault = decode_request::<Demo>(None, "tc").unwrap_err();
        assert_eq!(fault.name, "error/tc/invalid-request");
        assert!(fault.message.contains("no payload"), "{}", fault.message);
    }

    #[test]
    fn oversize_payload_is_rejected_before_parsing() {
        let big = zbytes(&vec![b'x'; MAX_REQUEST_PAYLOAD_BYTES + 1]);
        let fault = decode_request::<Demo>(Some(&big), "diagnostics").unwrap_err();
        assert_eq!(fault.name, "error/diagnostics/invalid-request");
        assert!(fault.message.contains("too large"), "{}", fault.message);
    }

    #[test]
    fn non_utf8_payload_is_a_modelled_fault() {
        let bad = zbytes(&[0xff, 0xfe, 0xfd]);
        let fault = decode_request::<Demo>(Some(&bad), "interface").unwrap_err();
        assert_eq!(fault.name, "error/interface/invalid-request");
        assert!(fault.message.contains("not UTF-8"), "{}", fault.message);
    }

    #[test]
    fn malformed_json_is_a_modelled_fault() {
        let bad = zbytes(b"{not json");
        let fault = decode_request::<Demo>(Some(&bad), "tc").unwrap_err();
        assert_eq!(fault.name, "error/tc/invalid-request");
        assert!(
            fault.message.contains("not valid JSON"),
            "{}",
            fault.message
        );
    }

    /// Well-formed JSON of the wrong shape is still a decode failure, not a
    /// panic and not a silent default.
    #[test]
    fn wrong_shape_json_is_a_modelled_fault() {
        let bad = zbytes(br#"{"something":"else"}"#);
        let fault = decode_request::<Demo>(Some(&bad), "tc").unwrap_err();
        assert_eq!(fault.name, "error/tc/invalid-request");
    }

    #[test]
    fn a_well_formed_request_decodes() {
        let ok = zbytes(br#"{"_wanted":7}"#);
        let decoded = decode_request::<Demo>(Some(&ok), "tc").expect("should decode");
        assert_eq!(decoded._wanted, 7);
    }

    /// The name must survive the round trip, because that is what tooling
    /// matches on.
    #[test]
    fn wire_format_carries_the_name_first() {
        let fault = QueryFault::invalid_request("tc", "bad".to_string());
        assert_eq!(fault.wire(), "error/tc/invalid-request: bad");
        assert!(fault.wire().starts_with(&fault.name));
    }
}
