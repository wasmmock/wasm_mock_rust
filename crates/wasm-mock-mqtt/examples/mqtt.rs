#![recursion_limit = "256"]
extern crate wapc_guest as guest;
extern crate wasm_mock_mqtt;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc,Mutex};
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate wasm_mock_macro;
use mqtt_v5::types::Packet;
use wasm_mock_macro::{test_suite,mock_suite};
use wasm_mock_util::*;
use base64::{Engine as _, engine::{general_purpose}};
use lazy_static::lazy_static;
use bytes::{BufMut, Bytes, BytesMut};
use std::error::Error;
#[no_mangle]
pub extern "C" fn _start() {
    mock_suite!{
        
        name my_test_suite;
        modify tcp_req "1884-:1883" (req){
            wasm_mock_mqtt::handle_req(&req, |packet:&mut Packet|{

            })
            //vec![TcpItem{Payload:req.Payload,String:format!("{:?}",debug),Id:String::from("1"),Laddr:req.Laddr,Raddr:req.Raddr}]
        }
        modify tcp_res "1884-:1883" (res) {
            wasm_mock_mqtt::handle_res(&res, |packet:&mut Packet|{
                
            })
            //Ok(vec![TcpItem{Payload:req.Payload,String:format!("{:?}",debug),Id:String::from("1"),Laddr:req.Laddr,Raddr:req.Raddr}])
        }
        modify tcp_replayer "1884-:1883" (res) {
          
        }
    }
}
fn main(){
    
}