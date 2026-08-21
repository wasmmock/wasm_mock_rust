//! A passthrough WebSocket fiddler for `3340-:3341`: records the frames between
//! octoscode and the `mock_octos` guest without altering either side.
//!
//! Same idea as `ws.rs`, one hop over. `ws.rs` sits in front of a real
//! `octos serve` (`3335-:3334`); this one sits in front of the MOCK, so the
//! whole path is wasm:
//!
//!     octoscode -> :3340 (this) -> :3341 (mock_octos) -> :20825 (decoy dial)
//!
//! Both halves of the port map must match the registered function names, so
//! they come from one macro rather than three string literals — a mismatch
//! means the host never invokes the guest and the connection simply hangs.
//!
//! ## Forwarding is not best-effort; recording is
//!
//! `handle_ws_req` returns `/continue` — forward NOTHING — whenever it cannot
//! re-encode what it decoded, and `process_handshake_req` takes that path any
//! time `finish_decoding()` fails on the client's upgrade. For a fiddler in
//! front of a real server that is merely a lost recording; here it swallows the
//! handshake outright, and the mock server logs
//!
//!     _modify_req... 3340-:3341 141 -> new request len: 0
//!     msgpack err BeforeReq msgpack: invalid code=10 decoding array length
//!
//! while the client waits forever for a 101 that is never going to arrive.
//!
//! So every path that would drop bytes falls back to handing the payload back
//! verbatim instead. `TcpPayload.Payload` is already the base64 of exactly
//! those bytes, so returning it is the identity — the connection behaves as if
//! the fiddler were not there, which is the worst it should ever degrade to.
extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
use base64::{engine::general_purpose, Engine as _};
use wasm_mock_util::*;
use wasm_mock_websocket::*;

macro_rules! port_map {
    () => {
        "3340-:3341"
    };
}

/// The host's sentinel for "forward nothing".
const CONTINUE: &[u8] = b"/continue";

/// Hand the payload straight back, unchanged.
///
/// A non-empty `Id` is what buys a row in the fiddler report — the host only
/// walks `TcpFiddlerMockWasmBeforeReq` for items that carry one, and
/// `/continue` skips that loop entirely, which is how a capture ends up a
/// `"tests": []` shell.
fn passthrough(tcp_payload: &TcpPayload) -> CallResult {
    let item = TcpItem {
        Payload: tcp_payload.Payload.clone(),
        String: String::new(),
        Id: format!("{}-{}", tcp_payload.Laddr, tcp_payload.Raddr),
        Laddr: tcp_payload.Laddr.clone(),
        Raddr: tcp_payload.Raddr.clone(),
    };
    Ok(rmp_serde::to_vec(&vec![item])?)
}

/// True when the helper decided to forward nothing.
///
/// Three shapes mean the same thing to the host, and only the first is obvious:
/// the `/continue` sentinel, no bytes at all, and `0x90` — msgpack for an EMPTY
/// array, which is what the frame path returns once the handshake is done and
/// the recording closure hands back no items. All three forward zero bytes, so
/// all three have to fall through to passthrough or the session dies the moment
/// the first JSON-RPC frame arrives.
const EMPTY_ARRAY: &[u8] = &[0x90];

fn dropped(result: &[u8]) -> bool {
    result.is_empty() || result == CONTINUE || result == EMPTY_ARRAY
}

fn _req(msg: &[u8]) -> CallResult {
    let tcp_payload: TcpPayload = rmp_serde::from_read_ref(msg)?;
    let c = |_c: &mut websocket_codec::Message| -> CallResult { Ok(vec![]) };
    //Origin is rewritten to the port the CLIENT dialled: it believes it is
    //talking to :3340, and an Origin naming the upstream is one a server is
    //entitled to reject.
    let result = handle_ws_req(&tcp_payload, "http://localhost:3340", c);
    //The fallback applies to the HANDSHAKE ONLY. "Forwarded nothing" is the
    //correct answer for a frame that is still incomplete — the helper has the
    //partial bytes buffered and will emit the whole frame when the tail lands.
    //Passing those same bytes through as well duplicates them, and the client
    //receives its replies chopped into fragments with frame headers embedded
    //mid-JSON. Only an upgrade that failed to re-encode is safe to hand back,
    //because nothing is buffered behind it.
    if is_upgrade(&tcp_payload) {
        //Always the original bytes, even when the rewrite SUCCEEDED.
        //`process_handshake_req` re-encodes the decoded request and returns only
        //that, so anything the client packed into the same TCP segment after the
        //header block is silently discarded. octoscode does exactly that: its
        //upgrade and its first request (`config/capabilities/list`, id tui-1)
        //arrive together, and losing that one frame leaves `state.capabilities`
        //None forever — the client only re-requests it on reconnect — so every
        //capability-gated command reports "Octos UI capabilities are not
        //available" while the session itself looks perfectly healthy.
        //
        //The rewrite is only an Origin swap and nothing downstream checks it, so
        //forwarding the segment verbatim costs nothing and keeps the frame.
        //`handle_ws_req` is still called for its side effect: the channel state
        //machine has to advance to Done or every later frame is treated as
        //handshake bytes.
        let _ = &result;
        return passthrough(&tcp_payload);
    }
    result
}

fn _res(msg: &[u8]) -> CallResult {
    let tcp_payload: TcpPayload = rmp_serde::from_read_ref(msg)?;
    //Passthrough: record the frame and leave it untouched, so the JSON-RPC
    //envelopes reach octoscode exactly as mock_octos wrote them. No fallback
    //here at all — every short return on this path is a partial frame.
    let c = |_c: &mut websocket_codec::Message| -> CallResult { Ok(vec![]) };
    handle_ws_res(&tcp_payload, c)
}

/// True when this payload opens a connection rather than continuing one.
fn is_upgrade(tcp_payload: &TcpPayload) -> bool {
    general_purpose::STANDARD
        .decode(tcp_payload.Payload.clone())
        .map(|p| p.starts_with(b"GET "))
        .unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn _start() {
    //the server invokes {port_map}_tcp_modify_req / _tcp_modify_res for TCP
    //fiddler targets; the _tcp infix is what routes 3340-:3341 to this wasm
    register_function(concat!(port_map!(), "_tcp_modify_req"), _req);
    register_function(concat!(port_map!(), "_tcp_modify_res"), _res);
}

fn main() {}
