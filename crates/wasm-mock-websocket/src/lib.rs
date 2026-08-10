use lazy_static::lazy_static;
use std::collections::HashMap;
use bytecodec::io::{ReadBuf};
use std::sync::{Mutex,Arc};
use tokio_util::codec::{Encoder,Decoder};
use wapc_guest::prelude::CallResult;
use websocket_codec::MessageCodec;
use bytes::BytesMut;
use base64::{Engine as _, engine::{general_purpose}};
use wasm_mock_util::{TcpPayload,TcpItem};
use std::io::Read;
use std::io::Cursor;
mod channel;
use channel::{Channel};
mod handshake;
use handshake::{Handshake,HandshakeRes};
lazy_static! {
    static ref CHANNEL_MAP: Arc<Mutex<HashMap<String,Channel>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

/// Handles conversion of tcp packets from local to remote connection into websocket framed messages
///
/// # Examples
///
/// ```
/// extern crate wapc_guest as guest;
/// use guest::prelude::*;
/// extern crate wasm_mock_util;
/// use wasm_mock_websocket::*;
/// use wasm_mock_util::*;
/// fn _req(msg: &[u8]) -> CallResult{
///     let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
///     let c = |_c: &mut websocket_codec::Message|->CallResult{
///          Ok(vec![])
///     };
///     //change origin from 3334 to 3335 ( as the page is served in localhost:3334, but the mock server dial from port 3335)
///     handle_ws_req(&tcp_payload,"http://localhost:3335",c)
/// }
/// fn _res(msg: &[u8]) -> CallResult{
///     let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
///     let c = |c: &mut websocket_codec::Message|->CallResult{
///         *c = websocket_codec::Message::text("echo");
///         Ok(vec![])
///     };
///     handle_ws_res(&tcp_payload,c)
/// }
/// #[no_mangle]
/// pub extern "C" fn _start() {
///     REGISTRY.lock().unwrap().insert("3335-:3334_req_json".into(),_req);
///     REGISTRY.lock().unwrap().insert("3335-:3334_res_json".into(),_res);
/// }
/// fn main(){
 
/// }
/// ```
///
/// # Arguments
///
/// * `tcp_payload` - TcpPayload
/// * `change_origin` - Change of http origin
/// * `c` - User defined closure to handle Websocket messages
///
/// # Returns
///
/// CallResult
pub fn handle_ws_req<F>(tcp_payload:&TcpPayload,change_origin:&str,c:F)->CallResult where F: Fn(&mut websocket_codec::Message)->CallResult{
    let mut p = CHANNEL_MAP.lock().unwrap();
    let conn = format!("{}-{}",tcp_payload.Laddr,tcp_payload.Raddr);
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    // CHANNEL_MAP is keyed on "{laddr}-{raddr}" and never evicted — the guest
    // only ever sees payloads, so it never learns that a connection closed. The
    // OS reuses ephemeral ports, so a NEW connection eventually lands on a stale
    // entry whose handshake is already Done; its HTTP upgrade is then fed to the
    // frame decoder as if it were WebSocket frames and the connection is wedged
    // for good — the handshake still returns 101 while nothing forwards, which
    // reads as "the proxy is up but the session never starts". A payload opening
    // with a request line can only be a fresh connection, so re-arm the channel
    // instead of inheriting the dead one's state.
    let is_fresh_upgrade = payload.starts_with(b"GET ");
    let mut file = Cursor::new(payload);
    if let Some(channel)= p.get_mut(&conn){
        if is_fresh_upgrade && matches!(channel.handshake, Handshake::Done){
            *channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        }
        channel.ws_reqbuf.fill(&mut file)?;
        let result = match channel.handshake{
            Handshake::RecvRequest(_)=>{
                channel.process_handshake_req(change_origin)
            },
            Handshake::Done=>{
                process_closure(&mut channel.ws_reqbuf,&mut channel.frame_req_decoder,true,&mut channel.req_pending,&mut channel.req_desynced,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        return result;
    }else{
        let mut channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        channel.ws_reqbuf.fill(&mut file)?;
        let result = match channel.handshake{
            Handshake::RecvRequest(_)=>{
                channel.process_handshake_req(change_origin)
            },
            Handshake::Done=>{
                process_closure(&mut channel.ws_reqbuf,&mut channel.frame_req_decoder,true,&mut channel.req_pending,&mut channel.req_desynced,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        p.insert(conn,channel);
        return result;
    }
}
/// Handles conversion of tcp packets from local to remote connection into websocket framed messages
///
/// # Examples
///
/// ```
/// extern crate wapc_guest as guest;
/// use guest::prelude::*;
/// extern crate wasm_mock_util;
/// use wasm_mock_websocket::*;
/// use wasm_mock_util::*;
/// fn _req(msg: &[u8]) -> CallResult{
///     let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
///     let c = |_c: &mut websocket_codec::Message|->CallResult{
///          Ok(vec![])
///     };
///     //change origin from 3334 to 3335 ( as the page is served in localhost:3334, but the mock server dial from port 3335)
///     handle_ws_req(&tcp_payload,"http://localhost:3335",c)
/// }
/// fn _res(msg: &[u8]) -> CallResult{
///     let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
///     let c = |c: &mut websocket_codec::Message|->CallResult{
///         *c = websocket_codec::Message::text("echo");
///         Ok(vec![])
///     };
///     handle_ws_res(&tcp_payload,c)
/// }
/// #[no_mangle]
/// pub extern "C" fn _start() {
///     REGISTRY.lock().unwrap().insert("3335-:3334_req_json".into(),_req);
///     REGISTRY.lock().unwrap().insert("3335-:3334_res_json".into(),_res);
/// }
/// fn main(){
 
/// }
/// ```
///
/// # Arguments
///
/// * `tcp_payload` - TcpPayload
/// * `c` - User defined closure to handle Websocket messages
///
/// # Returns
///
/// CallResult
pub fn handle_ws_res<F>(tcp_payload:&TcpPayload,c:F)->CallResult where F: Fn(&mut websocket_codec::Message)->CallResult{
    let mut p = CHANNEL_MAP.lock().unwrap();
    let conn = format!("{}-{}",tcp_payload.Laddr,tcp_payload.Raddr);
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    let mut file = Cursor::new(payload);
    if let Some(channel)= p.get_mut(&conn){
        channel.ws_resbuf.fill(&mut file)?;
        let result = match channel.handshake_res{
            HandshakeRes::RecvResponse(_)=>{
                channel.process_handshake_res(tcp_payload.Payload.clone())
            }
            HandshakeRes::Done=>{
                process_closure(&mut channel.ws_resbuf,&mut channel.frame_res_decoder,false,&mut channel.res_pending,&mut channel.res_desynced,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        return result;
    }else{
        let mut channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        channel.ws_resbuf.fill(&mut file)?;
        let result = match channel.handshake_res{
            HandshakeRes::RecvResponse(_)=>{
                channel.process_handshake_res(tcp_payload.Payload.clone())
            }
            HandshakeRes::Done=>{
                process_closure(&mut channel.ws_resbuf,&mut channel.frame_res_decoder,false,&mut channel.res_pending,&mut channel.res_desynced,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        p.insert(conn,channel);
        return result;
    }
}
/// Hand `buf` back for the host to forward verbatim.
///
/// `/continue` does NOT mean "forward the original" — the host only writes what
/// the guest returns (`tcpproxy/pool.go`: `if !bytes.Equal(mb, "/continue") {
/// write }`), so returning it DROPS the payload. That is correct while a
/// handshake is still being assembled and nothing should go out yet, but fatal
/// for bytes we merely chose not to rewrite: a 55 KB `session/open` reply split
/// across the host's 20000-byte reads lost every chunk that way, so the client
/// never opened a session.
///
/// An empty `Id` keeps these raw spans out of the report's RPC pairing — they
/// are stream bytes, not a decoded message.
fn passthrough(buf:&[u8],laddr:&str,raddr:&str)->CallResult{
    let item = TcpItem{
        Payload:general_purpose::STANDARD.encode(buf),
        String:String::new(),
        Id:String::new(),
        Laddr:laddr.to_string(),
        Raddr:raddr.to_string()
    };
    let out = rmp_serde::to_vec(&vec![item])?;
    Ok(out)
}

/// How many more bytes the LAST frame in `buf` still needs, walking frames from
/// a known-aligned start. `Some(0)` means the payload ends exactly on a frame
/// boundary; `None` means a frame header was itself truncated, which cannot be
/// resolved without buffering.
///
/// This exists because "the codec decoded something" is NOT evidence of
/// alignment: fed mid-message bytes it happily reads a length byte out of JSON
/// and returns a plausible frame. Rewriting on that basis chopped a large
/// `session/open` reply into a shower of ~100-byte fragments.
fn frame_alignment(buf: &[u8]) -> Option<usize> {
    let mut offset = 0usize;
    loop {
        if offset == buf.len() {
            return Some(0);
        }
        let rest = &buf[offset..];
        if rest.len() < 2 {
            return None;
        }
        let masked = rest[1] & 0x80 != 0;
        let len7 = (rest[1] & 0x7f) as usize;
        let (payload_len, mut header_len) = match len7 {
            126 => {
                if rest.len() < 4 {
                    return None;
                }
                (u16::from_be_bytes([rest[2], rest[3]]) as usize, 4)
            }
            127 => {
                if rest.len() < 10 {
                    return None;
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&rest[2..10]);
                (u64::from_be_bytes(bytes) as usize, 10)
            }
            other => (other, 2),
        };
        if masked {
            header_len += 4;
            if rest.len() < header_len {
                return None;
            }
        }
        let total = header_len + payload_len;
        if rest.len() < total {
            return Some(total - rest.len());
        }
        offset += total;
    }
}

fn process_closure<F>(read_buf:&mut ReadBuf<Vec<u8>>,frame_decoder:&mut MessageCodec,masked_encode:bool,pending:&mut usize,desynced:&mut bool,laddr:String,raddr:String,closure:F )->CallResult where
F: Fn(&mut websocket_codec::Message)->CallResult
{
    let mut consolidated = vec![];
    if read_buf.len()==0{
        return Ok(b"/continue".to_vec());
    }
    let mut buf:Vec<u8> =vec![];
    let r = read_buf.read_to_end(&mut buf);
    if r.is_ok(){
        // Alignment first: a payload may only be rewritten when it begins on a
        // frame boundary AND ends on one. Anything else is forwarded verbatim.
        if *desynced{
            return passthrough(&buf,&laddr,&raddr);
        }
        if *pending > 0{
            if buf.len() <= *pending{
                *pending -= buf.len();
                return passthrough(&buf,&laddr,&raddr);
            }
            // The tail of the in-flight frame ends inside this payload; realign
            // on what follows, but this payload still carries foreign bytes so
            // it goes out untouched.
            let offset = *pending;
            *pending = 0;
            match frame_alignment(&buf[offset..]){
                Some(remaining)=>{ *pending = remaining; }
                None=>{ *desynced = true; }
            }
            return passthrough(&buf,&laddr,&raddr);
        }
        match frame_alignment(&buf){
            Some(0)=>{}
            Some(remaining)=>{
                *pending = remaining;
                return passthrough(&buf,&laddr,&raddr);
            }
            None=>{
                *desynced = true;
                return passthrough(&buf,&laddr,&raddr);
            }
        }
        let mut bm = BytesMut::with_capacity(buf.len());
        bm.extend_from_slice(&buf);
        // Drain EVERY complete message in this payload. One TCP segment
        // routinely carries several WebSocket frames — on a fast turn the
        // reply deltas, the final answer, the usage report and the terminal
        // all arrive coalesced — and emitting only the first one silently
        // dropped the rest, so the peer never saw them. `decode` returns
        // Ok(None) on a partial frame and consumes nothing, so the loop
        // terminates with any incomplete tail still in `bm`.
        loop{
            if bm.is_empty(){
                break;
            }
            match frame_decoder.decode(&mut bm){
                Ok(Some(mut rr))=>{
                    closure(&mut rr)?;
                    let mut bytes = BytesMut::new();
                    match frame_decoder.encode(rr.clone(),&mut bytes){
                        Ok(_)=>{
                            let encoded_message = general_purpose::STANDARD.encode(bytes.to_vec());
                            consolidated.push(TcpItem{
                                Payload:encoded_message,
                                //as_text() is None for binary/ping/pong/close frames; an
                                //unwrap here traps the guest while CHANNEL_MAP is locked,
                                //poisoning the mutex so every later frame stalls
                                String:rr.as_text().map(str::to_string).unwrap_or_default(),
                                Id:format!("{}-{} ",laddr,raddr),
                                Laddr:laddr.clone(),
                                Raddr:raddr.clone()
                            });
                        }
                        //Re-encode failed: forward the ORIGINAL payload rather
                        //than a truncated rewrite of it.
                        _=>{
                            return passthrough(&buf,&laddr,&raddr);
                        }
                    }
                },
                //Partial frame (split across TCP segments) or undecodable
                //bytes: stop here and let the tail below carry them.
                _=>{
                    break;
                }
            }
        }
        //Leftover bytes mean a frame is split across TCP segments. We cannot
        //rewrite a payload we only half understand: emitting the decoded head
        //and appending the raw remainder desynchronises `MessageCodec` (it is
        //stateful), and every later payload then decodes as garbage — a big
        //`session/open` reply came out the far end as a shower of unparseable
        //fragments followed by a close. So rewrite ONLY payloads that consist
        //of whole frames; anything ragged is forwarded byte-for-byte and the
        //codec is reset so the next payload starts clean. The capture loses
        //split messages, the wire stays intact — the right trade for a proxy.
        if !bm.is_empty(){
            *frame_decoder = if masked_encode{
                MessageCodec::client()
            }else{
                MessageCodec::server()
            };
            return Ok(b"/continue".to_vec());
        }
    }
    if consolidated.len() >0{
        let buf = rmp_serde::to_vec(&consolidated)?;
        return Ok(buf);
    }
    return Ok(b"/continue".to_vec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::codec::Encoder;

    /// Encode `texts` as back-to-back server (unmasked) text frames, the way a
    /// burst of WebSocket messages arrives coalesced in one TCP segment.
    fn coalesced_payload(texts: &[&str]) -> Vec<u8> {
        let mut codec = MessageCodec::server();
        let mut out = BytesMut::new();
        for text in texts {
            codec
                .encode(websocket_codec::Message::text(*text), &mut out)
                .expect("encodes");
        }
        out.to_vec()
    }

    fn read_buf_of(payload: &[u8]) -> ReadBuf<Vec<u8>> {
        // Match the production channel buffer; a small one here silently
        // truncates the payload and fakes the very drift under test.
        let mut read_buf = ReadBuf::new(vec![0; 1 << 20]);
        read_buf
            .fill(&mut Cursor::new(payload.to_vec()))
            .expect("fills");
        read_buf
    }

    /// The host writes ONLY what the guest returns, so "forwarded untouched"
    /// means one item carrying the original bytes — never "/continue", which
    /// tells the host to send nothing.
    fn assert_forwarded_verbatim(result: &[u8], expected: &[u8]) {
        let items = items_of(result);
        assert_eq!(items.len(), 1, "one raw span");
        assert_eq!(
            general_purpose::STANDARD
                .decode(items[0].Payload.clone())
                .expect("base64"),
            expected,
            "payload forwarded byte-for-byte"
        );
        assert!(items[0].Id.is_empty(), "raw spans are not RPC pairs");
    }

    fn items_of(result: &[u8]) -> Vec<TcpItem> {
        rmp_serde::from_read_ref(result).expect("msgpack items")
    }

    #[test]
    fn forwards_every_frame_coalesced_into_one_payload() {
        let payload = coalesced_payload(&["first", "second", "third"]);
        let mut read_buf = read_buf_of(&payload);
        let mut codec = MessageCodec::server();

        let result = process_closure(
            &mut read_buf,
            &mut codec,
            false,
            &mut 0,
            &mut false,
            "laddr".into(),
            "raddr".into(),
            |_message| Ok(vec![]),
        )
        .expect("processes");

        let items = items_of(&result);
        let texts: Vec<&str> = items.iter().map(|item| item.String.as_str()).collect();
        assert_eq!(texts, vec!["first", "second", "third"]);
    }

    /// A payload we only half understand must be forwarded byte-for-byte, not
    /// rewritten. Emitting the decoded head and appending the raw remainder
    /// desynchronises the stateful `MessageCodec`, and everything after it
    /// decodes as garbage — a large `session/open` reply reached the client as
    /// a shower of unparseable fragments followed by a close.
    #[test]
    fn passes_through_a_payload_whose_last_frame_is_incomplete() {
        let whole = coalesced_payload(&["first"]);
        let split = coalesced_payload(&["second"]);
        let mut payload = whole.clone();
        payload.extend_from_slice(&split[..3]);

        let mut read_buf = read_buf_of(&payload);
        let mut codec = MessageCodec::server();

        let result = process_closure(
            &mut read_buf,
            &mut codec,
            false,
            &mut 0,
            &mut false,
            "laddr".into(),
            "raddr".into(),
            |_message| Ok(vec![]),
        )
        .expect("processes");

        assert_forwarded_verbatim(&result, &payload);
    }

    /// The host reads with a 20000-byte buffer (`tcpproxy/pool.go`), so a large
    /// message — a `session/open` reply carrying the workspace listing is ~55 KB
    /// — ALWAYS reaches the guest in several payloads. Every one of them must be
    /// forwarded untouched: rewriting any single chunk re-frames the message to
    /// the length of the fragment the guest could see, which is how the reply
    /// arrived truncated mid-JSON and the client never opened its session.
    #[test]
    fn never_rewrites_a_message_the_host_split_into_20000_byte_reads() {
        const HOST_READ: usize = 20000;
        let body = "x".repeat(55_000);
        let whole = coalesced_payload(&[body.as_str()]);
        assert!(whole.len() > 2 * HOST_READ, "needs to span several reads");

        let mut codec = MessageCodec::server();
        let mut pending = 0usize;
        let mut desynced = false;
        let mut forwarded = 0usize;

        for chunk in whole.chunks(HOST_READ) {
            let result = process_closure(
                &mut read_buf_of(chunk),
                &mut codec,
                false,
                &mut pending,
                &mut desynced,
                "laddr".into(),
                "raddr".into(),
                |_message| Ok(vec![]),
            )
            .expect("processes");

            assert_forwarded_verbatim(&result, chunk);
            forwarded += chunk.len();
        }

        assert_eq!(forwarded, whole.len(), "every byte forwarded exactly once");
        assert_eq!(pending, 0, "alignment restored once the message completes");
        assert!(!desynced, "a cleanly split message must not desync the channel");
    }

    #[test]
    #[ignore = "superseded by passes_through_a_payload_whose_last_frame_is_incomplete"]
    fn forwards_an_undecodable_tail_verbatim_after_the_frames_it_follows() {
        // One whole frame plus the first bytes of the next: a frame split
        // across TCP segments. The tail is still stream bytes — dropping it
        // corrupts the peer's next frame.
        let whole = coalesced_payload(&["first"]);
        let split = coalesced_payload(&["second"]);
        let tail = &split[..3];
        let mut payload = whole.clone();
        payload.extend_from_slice(tail);

        let mut read_buf = read_buf_of(&payload);
        let mut codec = MessageCodec::server();

        let result = process_closure(
            &mut read_buf,
            &mut codec,
            false,
            &mut 0,
            &mut false,
            "laddr".into(),
            "raddr".into(),
            |_message| Ok(vec![]),
        )
        .expect("processes");

        let items = items_of(&result);
        assert_eq!(items.len(), 2, "frame plus its trailing partial");
        assert_eq!(items[0].String, "first");
        assert_eq!(
            general_purpose::STANDARD
                .decode(items[1].Payload.clone())
                .expect("base64"),
            tail,
            "tail forwarded byte-for-byte"
        );
    }

    #[test]
    fn leaves_a_payload_with_no_complete_frame_untouched() {
        // Nothing decodable yet: "/continue" tells the host to forward the
        // original bytes. Emitting the tail here too would send them twice.
        let split = coalesced_payload(&["second"]);
        let mut read_buf = read_buf_of(&split[..3]);
        let mut codec = MessageCodec::server();

        let result = process_closure(
            &mut read_buf,
            &mut codec,
            false,
            &mut 0,
            &mut false,
            "laddr".into(),
            "raddr".into(),
            |_message| Ok(vec![]),
        )
        .expect("processes");

        assert_forwarded_verbatim(&result, &split[..3]);
    }
}
