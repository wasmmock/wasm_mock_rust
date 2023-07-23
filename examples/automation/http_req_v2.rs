extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate wasm_mock_macro;
extern crate http_req;
extern crate httparse;
use wasm_mock_macro::test_suite;
use wasm_mock_util::*;
use http_req::request;
use std::{ convert::TryFrom};
use http_req::{request::RequestBuilder, uri::Uri, response::StatusCode};
#[macro_use]
extern crate serde_json;
#[no_mangle]
pub extern "C" fn _start() {
    test_suite!{
        
        name my_test_suite;
        
        test http_get "/learn" ([httparse::Header{name:"",value:b""},
        httparse::Header{name:"",value:b""}])(res) {
            println!("ss");
        }
        
    }
}
fn main(){
 
}