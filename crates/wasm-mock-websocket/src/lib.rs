use lazy_static::lazy_static;
use std::collections::HashMap;
use bytecodec::io::{ReadBuf};
use std::sync::{Mutex,Arc};
use tokio_util::codec::{Encoder,Decoder};
use websocket_codec::MessageCodec;
use serde::{Deserialize, Serialize};
use bytes::BytesMut;
use base64::{Engine as _, engine::{general_purpose}};
use std::result;
use std::error::Error;
pub type Result<T> = result::Result<T, Box<dyn Error>>;
use std::io::Read;
use std::io::Cursor;
mod channel;
use channel::{Channel};
mod handshake;
use handshake::{Handshake,HandshakeRes};
lazy_static! {
    static ref CHANNEL_MAP: Arc<Mutex<HashMap<String,Channel>>> =
        Arc::new(Mutex::new(HashMap::new()));
}
/// A consolidated vector of TcpItems is marshalled (MessagePack) and sent to the mock server. The mock server will stream the data into the remote connection. It also contains meta information about it's connection
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct TcpItem {
    /// Base64 encoded string of framed messages
    pub Payload: String,
    /// Human readable representation of the payload for reporting
    pub String: String,
    /// If Id is not empty string, the reporting recognizes this TCP Request as RPC. Mock server expects a response with similar Id from remote connection
    pub Id: String,
    /// Local connection address
    pub Laddr: String,
    /// Remote connection address
    pub Raddr: String,
}
/// Standard TcpPayload struct used in unmarshalling of request (MessagePack) that contains tcp packet and meta information about it's connection
#[allow(non_snake_case)]
#[derive(Serialize,Deserialize,Debug)]
pub struct TcpPayload{
    /// Base64 encoded tcp packet
    pub Payload: String,
    /// Local connection port assigned by mock server
    pub Laddr: String,
    /// Remote connection port dialled from mock server
    pub Raddr: String,
}
/// Handles conversion of tcp packets from local to remote connection into websocket framed messages
///
/// # Examples
///
/// ```
/// use wasm_mock_websocket::handle_req;
/// #[no_mangle]
/// pub extern "C" fn wapc_init() {
///     register_function("3335-:3334_req_json",_req);
/// }
/// fn _req(msg: &[u8]) -> CallResult {
///     let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
///     let c = |c: &mut websocket_codec::Message|->Result<()>{
///         Ok(())
///     };
///     handle_req(&tcp_payload,"http://localhost:3334",c)
/// }
/// ```
///
/// # Arguments
///
/// * `tcp_payload` - TcpPayload
/// * `change_origin` - Change of http origin
/// * `c` - User defined closure to handle Websocket messages
///
/// # Returns
///
/// Result
pub fn handle_req<F>(tcp_payload:&TcpPayload,change_origin:&str,c:F)->Result<Vec<u8>> where F: Fn(&mut websocket_codec::Message)->Result<()>{
    let mut p = CHANNEL_MAP.lock().unwrap();
    let conn = format!("{}-{}",tcp_payload.Laddr,tcp_payload.Raddr);
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    let mut file = Cursor::new(payload);
    if let Some(channel)= p.get_mut(&conn){
        channel.ws_reqbuf.fill(&mut file)?;
        let result = match channel.handshake{
            Handshake::RecvRequest(_)=>{
                channel.process_handshake_req(change_origin)
            },
            Handshake::Done=>{
                process_closure(&mut channel.ws_reqbuf,&mut channel.frame_req_decoder,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        return result;
    }else{
        let mut channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        channel.ws_reqbuf.fill(&mut file)?;
        let result = match channel.handshake{
            Handshake::RecvRequest(_)=>{
                channel.process_handshake_req(change_origin)
            },
            Handshake::Done=>{
                process_closure(&mut channel.ws_reqbuf,&mut channel.frame_req_decoder,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        p.insert(conn,channel);
        return result;
    }
}
/// Handles conversion of tcp packets from local to remote connection into websocket framed messages
///
/// # Examples
///
/// ```
/// use wasm_mock_websocket::handle_res;
/// #[no_mangle]
/// pub extern "C" fn wapc_init() {
///     register_function("3335-:3334_res_json",_res);
/// }
/// fn _res(msg: &[u8]) -> CallResult {
///     let tcp_payload:TcpPayload = rmp_serde::from_read_ref(msg)?;
///     let c = |c: &mut websocket_codec::Message|->Result<()>{
///         Ok(())
///     };
///     handle_res(&tcp_payload,"http://localhost:3334",c)
/// }
/// ```
///
/// # Arguments
///
/// * `tcp_payload` - TcpPayload
/// * `c` - User defined closure to handle Websocket messages
///
/// # Returns
///
/// Result
pub fn handle_res<F>(tcp_payload:&TcpPayload,c:F)->Result<Vec<u8>> where F: Fn(&mut websocket_codec::Message)->Result<()>{
    let mut p = CHANNEL_MAP.lock().unwrap();
    let conn = format!("{}-{}",tcp_payload.Laddr,tcp_payload.Raddr);
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    let mut file = Cursor::new(payload);
    if let Some(channel)= p.get_mut(&conn){
        channel.ws_resbuf.fill(&mut file)?;
        let result = match channel.handshake_res{
            HandshakeRes::RecvResponse(_)=>{
                channel.process_handshake_res(tcp_payload.Payload.clone())
            }
            HandshakeRes::Done=>{
                process_closure(&mut channel.ws_resbuf,&mut channel.frame_res_decoder,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        return result;
    }else{
        let mut channel = Channel::new(tcp_payload.Laddr.clone(),tcp_payload.Raddr.clone());
        channel.ws_resbuf.fill(&mut file)?;
        let result = match channel.handshake_res{
            HandshakeRes::RecvResponse(_)=>{
                channel.process_handshake_res(tcp_payload.Payload.clone())
            }
            HandshakeRes::Done=>{
                process_closure(&mut channel.ws_resbuf,&mut channel.frame_res_decoder,channel.laddr.clone(),channel.raddr.clone(),c)
            }
        };
        p.insert(conn,channel);
        return result;
    }
}
fn process_closure<F>(read_buf:&mut ReadBuf<Vec<u8>>,frame_decoder:&mut MessageCodec,laddr:String,raddr:String,closure:F )->Result<Vec<u8>> where
F: Fn(&mut websocket_codec::Message)->Result<()>
{
    let mut consolidated = vec![];
    if read_buf.len()==0{
        return Ok(b"/continue".to_vec());
    }
    let mut buf:Vec<u8> =vec![];
    let r = read_buf.read_to_end(&mut buf);
    let buf_len = buf.len();
    if r.is_ok(){
        let mut bm = BytesMut::with_capacity(0);
        bm.extend_from_slice(&buf);
        let result = frame_decoder.decode(&mut bm);
        if let Some(f_l) = frame_decoder.frame_length{
            if f_l > buf_len{
            }else{
                match result{
                    Ok(r)=>{
                        if let Some(mut rr) = r{
                            //track_assert_eq!(rr.as_text(),None,ErrorKind::InvalidInput);
                            closure(&mut rr)?;
                            let mut bytes = BytesMut::new();
                            match frame_decoder.encode(rr.clone(),&mut bytes){
                                Ok(_)=>{
                                    let encoded_message = general_purpose::STANDARD.encode(bytes.to_vec());
                                    consolidated.push(TcpItem{
                                        Payload:encoded_message,
                                        String:rr.as_text().unwrap().to_string(),
                                        Id:format!("{}-{} ",laddr,raddr),
                                        Laddr:laddr.clone(),
                                        Raddr:raddr.clone()
                                    });
                                }
                                _=>{}
                            }
                            // track_assert_eq!(Some(format!("len.. {:?}",bytes.len())),None,ErrorKind::InvalidInput);
                            
                            
                        }
                    },
                    Err(_)=>{}
                }
                
            }
        }
    }
    if consolidated.len() >0{
        let buf = rmp_serde::to_vec(&consolidated)?;
        return Ok(buf);
    }
    return Ok(b"/continue".to_vec());
}
