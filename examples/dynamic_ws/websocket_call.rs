extern crate wapc_guest as guest;
use guest::prelude::*;
#[macro_use]
extern crate wasm_mock_util;
use wasm_mock_websocket::*;
use wasm_mock_util::*;
fn get_uid(_msg: &[u8]) -> CallResult {
    let mut uid = vec![];
    unsafe {
        uid = UID.clone();
    }
    foo_websocket_call!("get_uid", &uid)
}
fn command(_msg: &[u8]) -> CallResult {
    foo_websocket_call!("command", b"")
}
fn req(_msg: &[u8])->CallResult{
    foo_websocket_call!("request", b"")
}
fn request_marshalling(msg: &[u8]) -> CallResult {
    foo_websocket_call!("request_marshalling", msg)
}
fn response_marshalling(msg: &[u8]) -> CallResult {
    foo_websocket_call!("response_marshalling", msg)
}
#[no_mangle]
pub extern "C" fn _start() {
    REGISTRY.lock().unwrap().insert("get_uid".into(),get_uid);
    REGISTRY.lock().unwrap().insert("command".into(),command);
    REGISTRY.lock().unwrap().insert("request".into(),req);
    REGISTRY.lock().unwrap().insert("request_marshalling".into(),request_marshalling);
    REGISTRY.lock().unwrap().insert("response_marshalling".into(),response_marshalling);
}
fn main(){

}