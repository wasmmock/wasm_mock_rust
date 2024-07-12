use mqtt_v5::codec::MqttCodec;
use bytes::{BufMut, BytesMut};
use wapc_guest::prelude::CallResult;
use lazy_static::lazy_static;
use std::sync::{Arc,Mutex};
use std::collections::HashMap;
use wasm_mock_util::*;
pub use mqtt_v5::types::Packet;
lazy_static! {
    static ref CHANNEL_MAP: Arc<Mutex<HashMap<String,Channel>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref Count: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
}

use base64::{Engine as _, engine::{general_purpose}};
pub struct Channel{
    pub reqbuf: BytesMut,
    pub resbuf: BytesMut,
    pub frame_req_decoder: MqttCodec,
    pub frame_res_decoder: MqttCodec,
    pub laddr:String,
    pub raddr:String,
}
impl Channel{
    pub fn new(laddr:String,raddr:String) -> Self {
        Channel{
            reqbuf: BytesMut::new(),
            resbuf: BytesMut::new(),
            frame_req_decoder: MqttCodec::new(),
            frame_res_decoder:MqttCodec::new(),
            laddr:laddr,
            raddr:raddr
        }
    }
    
    
}
pub fn handle_req<F>(tcp_payload:&TcpPayload,c:F)->Result<Vec<TcpItem>,Box<dyn std::error::Error + Send + Sync>> where F: Fn( &mut mqtt_v5::types::Packet){
    let mut p = CHANNEL_MAP.lock().unwrap();
    let conn = format!("{}-{}",tcp_payload.Laddr,tcp_payload.Raddr);
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    let mut consolidated = vec![];
    if let Some(channel)= p.get_mut(&conn){
        channel.reqbuf.put_slice(&payload);
        if let Ok(Some(mut packet)) = channel.frame_req_decoder.decode(&mut channel.reqbuf){
            //c(&mut packet)
            c(&mut packet);
            let mut b = BytesMut::new();
            let s = format!("{:?}",packet);
            channel.frame_req_decoder.encode(packet, &mut b).unwrap();
            consolidated.push(TcpItem{Payload:general_purpose::STANDARD.encode(b),String:s,Id:format!("{:?}",Count.lock().unwrap()),Laddr:tcp_payload.Laddr.clone(),Raddr:tcp_payload.Raddr.clone()});
        }
    }else{
        let mut channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        channel.reqbuf.put_slice(&payload);
        if let Ok(Some(mut packet)) = channel.frame_req_decoder.decode(&mut channel.reqbuf){
            c(&mut packet);
            let mut b = BytesMut::new();
            let s = format!("{:?}",packet);
            channel.frame_req_decoder.encode(packet, &mut b).unwrap();
            consolidated.push(TcpItem{Payload:general_purpose::STANDARD.encode(b),String:s,Id:format!("{:?}",Count.lock().unwrap()),Laddr:tcp_payload.Laddr.clone(),Raddr:tcp_payload.Raddr.clone()});
        }
        p.insert(conn, channel);
    }
    Ok(consolidated)
}
pub fn handle_res<F>(tcp_payload:&TcpPayload,c:F)->Result<Vec<TcpItem>,Box<dyn std::error::Error + Send + Sync>> where F: Fn( &mut mqtt_v5::types::Packet){
    let mut p = CHANNEL_MAP.lock().unwrap();
    let conn = format!("{}-{}",tcp_payload.Laddr,tcp_payload.Raddr);
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    let mut consolidated = vec![];
    if let Some(channel)= p.get_mut(&conn){
        channel.resbuf.put_slice(&payload);
        if let Ok(Some(mut packet)) = channel.frame_res_decoder.decode(&mut channel.resbuf){
            //c(&mut packet)
            c(&mut packet);
            let mut b = BytesMut::new();
            let s = format!("{:?}",packet);
            channel.frame_res_decoder.encode(packet, &mut b).unwrap();
            consolidated.push(TcpItem{Payload:general_purpose::STANDARD.encode(b),String:s,Id:format!("{:?}",Count.lock().unwrap()),Laddr:tcp_payload.Laddr.clone(),Raddr:tcp_payload.Raddr.clone()});
        }
    }else{
        let mut channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        channel.resbuf.put_slice(&payload);
        if let Ok(Some(mut packet)) = channel.frame_res_decoder.decode(&mut channel.resbuf){
            c(&mut packet);
            let mut b = BytesMut::new();
            let s = format!("{:?}",packet);
            channel.frame_res_decoder.encode(packet, &mut b).unwrap();
            consolidated.push(TcpItem{Payload:general_purpose::STANDARD.encode(b),String:s,Id:format!("{:?}",Count.lock().unwrap()),Laddr:tcp_payload.Laddr.clone(),Raddr:tcp_payload.Raddr.clone()});
        }
        p.insert(conn, channel);
    }
    *Count.lock().unwrap() +=1;
    Ok(consolidated)
}
