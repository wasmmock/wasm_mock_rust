#[macro_use]
extern crate wasm_mock_macro;
extern crate wasm_mock_codegen;
use wasm_mock_codegen::{http_modify_req};
use wasm_mock_macro::test_suite;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
lazy_static!{
    pub static ref REGISTRY: Arc<Mutex<HashMap<String,fn(msg:&[u8])->CallResult>>> = Arc::new(Mutex::new(HashMap::new()));
}
fn z(i:i32) ->i32{
    3
}

fn main() {
    test_suite! {
        // for anonymous test suites remove the name directive
        name my_test_suite;
        // instead of `fn`, `test` defines a test item.
        // modify tcp_req "3335":-"3334" (i:i32) ->i32 {
        //     println!("simple_first_test2");
        //     z(i)
        // }
        modify http_req "simple_first_test"(i:i32) ->i32 {
            println!("simple_first_test");
            2
        }
        
    }
    println!("getting");
    let z = REGISTRY.lock().unwrap().get("3335:-3334").unwrap()(2);
    println!("z {:?}",z);
}
