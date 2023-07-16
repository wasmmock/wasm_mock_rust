extern crate wapc_guest as guest;
#[macro_use]
extern crate wasm_mock_util;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate wasm_mock_macro;
use byteorder::{ByteOrder, LittleEndian};
use guest::prelude::*;
use  wasm_mock_util::*;
use std::collections::HashMap;
use std::error::Error;
use wasm_mock_macro::test_suite;
fn a()->CallResult{
    Ok(Vec![])
}
#[no_mangle]
pub extern "C" fn _start() {
    test_suite!{
        name my_test_suite;
        modify http_req "/t.json" (req:*mut RequestReceivedInMock) {
            println!("simple_first_test");
            req.HttpProxyUrl = String::from("localhost:8000");
            req.HttpScheme = String::from("http");
        }
        modify http_res "/t.json" (res:*mut HttpResponse) {
            println!("simple_first_test");
            res.HttpBody = json!({"data":"hi"});
            res.StatusCode = String::from("200");
        }
    }
}
fn main(){
  
}