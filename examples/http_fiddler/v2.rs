extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
use wasm_mock_util::*;
#[macro_use]
extern crate wasm_mock_macro;
use wasm_mock_macro::test_suite;
#[macro_use]
extern crate serde_json;
#[no_mangle]
pub extern "C" fn _start() {
    test_suite!{
        name my_test_suite;
        // instead of `fn`, `test` defines a test item.
        // modify tcp_req "3335":-"3334" (i:i32) ->i32 {
        //     println!("simple_first_test2");
        //     z(i)
        // }
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