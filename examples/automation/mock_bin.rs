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
        host "http://mockbin.org";
        //test http_get ${path} $httparse's header $httpBody
        test http_get "/bin/9638de65-a0e6-4abd-9394-eba88fea447f" ([])(b"".to_vec())(res) {
           foo_assert_eq!(res.HttpBody.get("data").unwrap_or(&NULL),&json!("hi"),"data");
        }
        
    }
    
}
fn main(){
 
}