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
    let c = |c: &mut websocket_codec::Message|->CallResult{
        *c = websocket_codec::Message::text("echo");
        Ok(vec![])
    };
    handle_ws_res(&tcp_payload,c)
}
#[no_mangle]
pub extern "C" fn _start() {
    REGISTRY.lock().unwrap().insert("3335-:3334_req_json".into(),_req);
    REGISTRY.lock().unwrap().insert("3335-:3334_res_json".into(),_res);
}
fn main(){
 
}