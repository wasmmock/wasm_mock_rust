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
            req.HttpProxyUrl = String::from("localhost:8000");
            req.HttpScheme = String::from("http");
        }
        modify http_res "/t.json" (res) {
            res.HttpBody = json!({"data":"hi"});
            res.StatusCode = String::from("200");
        }
        modify http_replayer "/t.json" (res) {
            foo_assert_eq!(res.ResA.HttpBody.get("abc"),res.ResB.HttpBody.get("abc"),"abc");
            
        }
    }
    test_suite!{
        
        name my_test_suite;
        host "http://localhost:8000";
        //test http_get ${path} $httparse header $httpBody
        test http_get "/t.json" ([])(b"".to_vec())(res) {
           foo_assert_eq!(res.HttpBody.get("data").unwrap_or(&NULL),&json!("hi"),"data");
        }
        
    }
}
fn main(){
 
}