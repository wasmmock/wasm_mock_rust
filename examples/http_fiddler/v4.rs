#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2018::*;
#[macro_use]
extern crate std;
extern crate wapc_guest as guest;
#[macro_use]
extern crate wasm_mock_util;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate wasm_mock_macro;
use byteorder::{ByteOrder, LittleEndian};
use guest::prelude::*;
use wasm_mock_util::*;
use std::collections::HashMap;
use std::error::Error;
use wasm_mock_macro::test_suite;
fn a() -> CallResult {
    Ok(::alloc::vec::Vec::new())
}
#[no_mangle]
pub extern "C" fn _start() {
    REGISTRY
        .lock()
        .unwrap()
        .insert(
            {
                let res = ::alloc::fmt::format(
                    format_args!("{0}_http_modify_req", "/t.json"),
                );
                res
            },
            |msg: &[u8]| -> CallResult {
                let test_case_failed = ::std::cell::Cell::new(false);
                let mut req = foo_unmarshall::<RequestReceivedInMock>(msg)?;
                {
                    {
                        ::std::io::_print(format_args!("simple_first_test\n"));
                    };
                    req.HttpProxyUrl = String::from("localhost:8000");
                    req.HttpScheme = String::from("http");
                };
                let request = serde_json::to_string(&req)?;
                Ok(request.as_bytes().to_vec())
            },
        );
    REGISTRY
        .lock()
        .unwrap()
        .insert(
            {
                let res = ::alloc::fmt::format(
                    format_args!("{0}_modify_res", "/t.json"),
                );
                res
            },
            |msg: &[u8]| -> CallResult {
                let test_case_failed = ::std::cell::Cell::new(false);
                let mut res = foo_unmarshall::<HttpResponse>(msg)?;
                {
                    {
                        ::std::io::_print(format_args!("simple_first_test\n"));
                    };
                    res
                        .HttpBody = ::serde_json::Value::Object({
                        let mut object = ::serde_json::Map::new();
                        let _ = object
                            .insert(
                                ("data").into(),
                                ::serde_json::to_value(&"hi").unwrap(),
                            );
                        object
                    });
                    res.StatusCode = String::from("200");
                };
                let request = serde_json::to_string(&res)?;
                Ok(request.as_bytes().to_vec())
            },
        );
}
fn main() {}
