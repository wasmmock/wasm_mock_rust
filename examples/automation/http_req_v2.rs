extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate wasm_mock_macro;
extern crate httparse;
use wasm_mock_macro::test_suite;
use wasm_mock_util::*;

#[macro_use]
extern crate serde_json;
use serde_json::Value::Null as NULL;
#[no_mangle]
pub extern "C" fn _start() {
    test_suite!{
        
        name my_test_suite;
        host "http://localhost:8000";
        //test http_get ${path} $httparse header $httpBpdy
        test http_get "/t.json" ([])(b"".to_vec())(res) {
           foo_assert_eq!(res.HttpBody.get("data").unwrap_or(&NULL),&json!("hi"),"data");
        }
        // test http_get "/learn" ([httparse::Header{name:"",value:b""},
        // httparse::Header{name:"",value:b""}])(b"".to_vec())(res) {
        //     println!("ss");
        // }
    }
    
}
fn main(){
 
}