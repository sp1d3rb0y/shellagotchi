//! Wire-format types for the newline-delimited JSON IPC protocol exchanged
//! between the shellagotchi CLI/shell-hook and the daemon.
//!
//! Each message (request or response) is serialized as a single line of
//! JSON, terminated by `\n`. This module defines only the message shapes;
//! actual socket I/O (reading/writing lines, connection handling) is a
//! later task.
//!
//! # Request wire format
//!
//! ```jsonc
//! {"v":1,"op":"feed","exit_code":0,"duration_ms":142,"argv0":"cargo","ts":1770000000}
//! {"v":1,"op":"status"}
//! {"v":1,"op":"prompt","format":"compact"}
//! {"v":1,"op":"clean"}
//! {"v":1,"op":"pet"}
//! {"v":1,"op":"hatch","species":"blob"}
//! {"v":1,"op":"ping"}
//! ```
//!
//! # Response wire format
//!
//! ```jsonc
//! {"ok":true,"state":{ /* PetState */ }}
//! {"ok":true,"prompt":"🥚 ^_^ 82%"}
//! {"ok":false,"error":"pet is dead; run `shellagotchi hatch`"}
//! ```
//!
//! # Serde design notes
//!
//! `Request` wraps a version field (`v`) alongside an internally-tagged
//! (`#[serde(tag = "op")]`) enum `RequestOp`, flattened into the same JSON
//! object. This produces exactly the documented shape: `{"v":1,"op":"feed",
//! ...op-specific fields}`.
//!
//! Unknown/future ops must not hard-fail deserialization (an older daemon
//! talking to a newer client shouldn't crash the connection). Internally
//! tagged enums support this via `#[serde(other)]` on a unit fallback
//! variant (`RequestOp::Unknown`) — this is one of the narrow cases where
//! `#[serde(other)]` is legal on tagged enums, precisely because the
//! fallback variant carries no data.
//!
//! `Response` is deliberately NOT an untagged/tagged enum. Mixing
//! `#[serde(flatten)]`, `#[serde(tag = ...)]`, and `#[serde(untagged)]`
//! turned out to be extremely finicky to get to compile while producing
//! exactly the flat wire shape above (in particular, untagged enums with
//! flattened struct variants don't reliably produce clean `{"ok":true}`
//! with no leaked null fields). Instead `Response` is a single flat struct
//! with `ok: bool` plus `Option<...>` fields for `state`/`prompt`/`error`,
//! each marked `#[serde(skip_serializing_if = "Option::is_none")]` (so
//! absent fields never appear in serialized JSON) and `#[serde(default)]`
//! (so absent fields deserialize to `None` rather than erroring). This is
//! simpler, compiles cleanly, and matches the wire format exactly.
//! Convenience constructors (`ok_empty`, `ok_state`, `ok_prompt`, `err`)
//! keep call sites ergonomic despite the flat representation.

use serde::{Deserialize, Serialize};

use crate::pet::state::PetState;

/// The current IPC protocol version, sent as the `v` field on every
/// request.
pub const PROTOCOL_VERSION: u32 = 1;

/// A request sent from the CLI/shell-hook to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub v: u32,
    #[serde(flatten)]
    pub op: RequestOp,
}

impl Request {
    pub fn new(op: RequestOp) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            op,
        }
    }
}

/// The operation-specific payload of a [`Request`], tagged on the `op`
/// field of the enclosing JSON object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RequestOp {
    Feed {
        exit_code: i32,
        duration_ms: u64,
        argv0: String,
        ts: i64,
    },
    Status,
    Prompt {
        format: String,
    },
    Clean,
    Pet,
    Hatch {
        species: String,
    },
    Ping,
    /// Fallback for ops this build of the daemon/client doesn't recognize
    /// (e.g. a newer client talking to an older daemon). Deserializing an
    /// unknown `op` value must never hard-error the connection.
    #[serde(other)]
    Unknown,
}

/// A response sent from the daemon back to the CLI/shell-hook.
///
/// Represented as a single flat struct (rather than a tagged/untagged
/// enum) so that serialization always produces exactly the documented
/// wire shape with no leaked `null` fields. See the module-level docs for
/// why this shape was chosen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PetState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    /// A bare success acknowledgement with no extra payload, e.g. for
    /// `ping`/`clean`/`pet`: `{"ok":true}`.
    pub fn ok_empty() -> Self {
        Self {
            ok: true,
            state: None,
            prompt: None,
            error: None,
        }
    }

    /// A success response carrying a `PetState` snapshot, e.g. for
    /// `status`/`hatch`: `{"ok":true,"state":{...}}`.
    pub fn ok_state(state: PetState) -> Self {
        Self {
            ok: true,
            state: Some(state),
            prompt: None,
            error: None,
        }
    }

    /// A success response carrying a rendered prompt string, e.g. for
    /// `prompt`: `{"ok":true,"prompt":"..."}`.
    pub fn ok_prompt(prompt: String) -> Self {
        Self {
            ok: true,
            state: None,
            prompt: Some(prompt),
            error: None,
        }
    }

    /// An error response: `{"ok":false,"error":"..."}`.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            state: None,
            prompt: None,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::state::Species;
    use chrono::{TimeZone, Utc};

    fn fixed_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn feed_request_matches_wire_format() {
        let req = Request::new(RequestOp::Feed {
            exit_code: 0,
            duration_ms: 142,
            argv0: "cargo".into(),
            ts: 1_770_000_000,
        });

        let json = serde_json::to_string(&req).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["v"], 1);
        assert_eq!(value["op"], "feed");
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["duration_ms"], 142);
        assert_eq!(value["argv0"], "cargo");
        assert_eq!(value["ts"], 1_770_000_000);
    }

    #[test]
    fn all_simple_ops_roundtrip() {
        let cases = [
            (RequestOp::Status, "status"),
            (RequestOp::Clean, "clean"),
            (RequestOp::Pet, "pet"),
            (RequestOp::Ping, "ping"),
        ];

        for (op, expected_op_str) in cases {
            let req = Request::new(op.clone());
            let json = serde_json::to_string(&req).unwrap();

            let back: Request = serde_json::from_str(&json).unwrap();
            assert_eq!(req, back);

            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(value["op"], expected_op_str);
        }
    }

    #[test]
    fn prompt_request_roundtrips_with_format_field() {
        let req = Request::new(RequestOp::Prompt {
            format: "compact".into(),
        });

        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["op"], "prompt");
        assert_eq!(value["format"], "compact");
    }

    #[test]
    fn hatch_request_roundtrips_with_species_field() {
        let req = Request::new(RequestOp::Hatch {
            species: "dragon".into(),
        });

        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["op"], "hatch");
        assert_eq!(value["species"], "dragon");
    }

    #[test]
    fn unknown_op_deserializes_gracefully() {
        let json = r#"{"v":1,"op":"some_future_op","whatever":true}"#;
        let req: Request = serde_json::from_str(json).expect("unknown op must not hard-error");
        assert_eq!(req.op, RequestOp::Unknown);
    }

    #[test]
    fn ok_state_response_matches_wire_format() {
        let state = PetState::newborn("Rusty".into(), Species::Blob, fixed_now());
        let resp = Response::ok_state(state);

        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["ok"], true);
        assert!(value["state"].is_object());
        assert_eq!(value["state"]["schema_version"], 1);
    }

    #[test]
    fn ok_prompt_response_matches_wire_format() {
        let resp = Response::ok_prompt("🥚 ^_^ 82%".to_string());

        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["ok"], true);
        assert_eq!(value["prompt"], "🥚 ^_^ 82%");
    }

    #[test]
    fn err_response_matches_wire_format() {
        let resp = Response::err("pet is dead; run `shellagotchi hatch`");

        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "pet is dead; run `shellagotchi hatch`");
    }

    #[test]
    fn ok_empty_response_matches_wire_format() {
        let resp = Response::ok_empty();

        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["ok"], true);
        let obj = value.as_object().unwrap();
        assert_eq!(
            obj.keys().collect::<std::collections::HashSet<_>>(),
            std::collections::HashSet::from([&"ok".to_string()])
        );
    }
}
