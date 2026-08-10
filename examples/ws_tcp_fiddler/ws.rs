extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
use wasm_mock_websocket::*;
use wasm_mock_util::*;
fn _req(msg: &[u8]) -> CallResult{
    let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
    let c = |_c: &mut websocket_codec::Message|->CallResult{
         Ok(vec![])
    };
    //change origin from 3334 to 3335 ( as the page is served in localhost:3334, but the mock server dial from port 3335)
    handle_ws_req(&tcp_payload,"http://localhost:3335",c)
}
fn _res(msg: &[u8]) -> CallResult{
    let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
    //passthrough: record the frame but leave it untouched, so JSON-RPC
    //payloads (e.g. octos UI Protocol envelopes) reach the client intact
    let c = |_c: &mut websocket_codec::Message|->CallResult{
        Ok(vec![])
    };
    handle_ws_res(&tcp_payload,c)
}
#[no_mangle]
pub extern "C" fn _start() {
    //the server invokes {port_map}_tcp_modify_req / _tcp_modify_res for TCP
    //fiddler targets; the _tcp infix is what routes 3335-:3334 to this wasm
    register_function("3335-:3334_tcp_modify_req",_req);
    register_function("3335-:3334_tcp_modify_res",_res);
}
fn main(){
 
}