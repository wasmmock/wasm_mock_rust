#![recursion_limit = "256"]
extern crate wapc_guest as guest;
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate wasm_mock_macro;
use wasm_mock_macro::{test_suite,mock_suite};
use wasm_mock_util::*;
use base64::{Engine as _, engine::{general_purpose}};
use lazy_static::lazy_static;
lazy_static!{
    pub static ref CAP :Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![]));
}
#[no_mangle]
pub extern "C" fn _start() {
    mock_suite!{
        
        name my_test_suite;
        modify tcp_req "1884-:1883" (req) {
            // let b = general_purpose::STANDARD.decode(&req.Payload).unwrap_or(Vec::new());
            // *CAP.lock().unwrap()= b;
        }
        modify tcp_res "1884-:1883" (res) {
          
        }
        modify tcp_replayer "1884-:1883" (res) {
          foo_assert!(false,format!("req {:?}",*CAP.lock().unwrap()));
          
        }
    }
}
fn main(){
    
}