extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate wasm_mock_macro;
use wasm_mock_macro::{test_suite,mock_suite};
use wasm_mock_util::*;
use wasm_mock_util::RequestReceivedInMock;

#[macro_use]
extern crate serde_json;
use serde_json::Value::Null as NULL;
#[no_mangle]
pub extern "C" fn _start() {
    mock_suite!{
        
        name my_test_suite;
        
        modify http_req "/t.json" (req) {
            //new server https://64d0b47ed144564ea9f6228c--moonlit-parfait-ccb30a.netlify.app
            //old server regression
            req.HttpProxyUrl = String::from("64d0b4f7d144564f0cf6237f--steady-kelpie-166a13.netlify.app");
            req.HttpScheme = String::from("https");
            //req.HttpProxyUrl = String::from("localhost:8000");
            //req.HttpScheme = String::from("http");
        }
        modify http_res "/t.json" (res) {
            // res.HttpBody = json!({"data":"hi"});
            // res.StatusCode = String::from("200");
        }
        modify http_replayer "/t.json" (res) {
            foo_assert_eq!(res.ResA.HttpBody.get("data"),res.ResB.HttpBody.get("data"),"data");
            
        }
    }
}
fn main(){
 
}