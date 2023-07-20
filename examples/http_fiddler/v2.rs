extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate wasm_mock_macro;
use wasm_mock_macro::mock_suite;
use wasm_mock_util::*;
use wasm_mock_util::RequestReceivedInMock;

#[macro_use]
extern crate serde_json;
#[no_mangle]
pub extern "C" fn _start() {
    mock_suite!{
        
        name my_test_suite;
        
        modify http_req "/t.json" (req) {
            println!("simple_first_test");
            req.HttpProxyUrl = String::from("localhost:8000");
            req.HttpScheme = String::from("http");
        }
        modify http_res "/t.json" (res) {
            println!("simple_first_test");
            res.HttpBody = json!({"data":"hi"});
            res.StatusCode = String::from("200");
        }
    }
}
fn main(){
 
}