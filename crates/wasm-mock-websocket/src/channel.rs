use bytecodec::io::{IoDecodeExt, IoEncodeExt, ReadBuf};
use httpcodec::{RequestDecoder,BodyDecoder,ResponseDecoder,RequestEncoder,NoBodyEncoder,StatusCode,Request,HeaderField};
use websocket_codec::MessageCodec;
use bytecodec::{Encode,Decode};
use bytecodec::bytes::RemainingBytesDecoder;
use base64::{Engine as _, engine::{general_purpose}};
use crate::handshake::{Handshake,HandshakeRes};
use crate::{TcpItem};
use std::mem;
use crate::Result;
const BUF_SIZE: usize = 4096;
pub struct Channel{
    pub ws_reqbuf: ReadBuf<Vec<u8>>,
    pub ws_resbuf: ReadBuf<Vec<u8>>,
    pub handshake: Handshake,
    pub handshake_res: HandshakeRes,
    pub frame_req_decoder: MessageCodec,
    pub frame_res_decoder: MessageCodec,
    pub laddr:String,
    pub raddr:String,
}
impl Channel{
    pub fn new(laddr:String,raddr:String) -> Self {
        Channel{
            ws_reqbuf: ReadBuf::new(vec![0;BUF_SIZE]),
            ws_resbuf: ReadBuf::new(vec![0;BUF_SIZE]),
            handshake: Handshake::RecvRequest(RequestDecoder::default()),
            handshake_res: HandshakeRes::RecvResponse( ResponseDecoder::<BodyDecoder<RemainingBytesDecoder>>::default()),
            frame_req_decoder: MessageCodec::client(),
            frame_res_decoder: MessageCodec::server(),
            laddr:laddr,
            raddr:raddr
        }
    }
}
impl Channel{
    pub fn process_handshake_req(&mut self,origin:&str)->Result<Vec<u8>>{
        match mem::replace(&mut self.handshake, Handshake::Done){
            Handshake::RecvRequest(mut decoder)=>{
                let result = decoder.decode_from_read_buf(&mut self.ws_reqbuf);
                if result.is_ok() && !decoder.is_idle() {
                    self.handshake = Handshake::RecvRequest(decoder);
                    return Ok(b"/continue".to_vec());
                }
                match decoder.finish_decoding(){
                    Ok(request)=>{
                        //track_assert_eq!(Some("finish_decoding"),None,ErrorKind::InvalidInput);
                        let new_req = modify_request_origin(&request,origin) ;
                        let mut encoder: RequestEncoder<NoBodyEncoder>= RequestEncoder::default();
                        encoder.start_encoding(new_req).unwrap();
                        let mut buf = Vec::new();
                        encoder.encode_all(&mut buf).unwrap();
                        let mut consolidated:Vec<TcpItem> = vec![];
                        let item = TcpItem{
                            Payload:general_purpose::STANDARD.encode(buf.clone()),
                            String:std::str::from_utf8(&buf).unwrap().to_string(),
                            Id:format!("{}-{}:Handshake",self.laddr,self.raddr),
                            Laddr:self.laddr.clone(),
                            Raddr:self.raddr.clone()
                        };
                        consolidated.push(item);
                        let buf = rmp_serde::to_vec(&consolidated)?;
                        Ok(buf)
                    },
                    Err(_)=>{
                        //track_assert_eq!(Some(format!("finish_decoding not {:?}",e)),None,ErrorKind::InvalidInput);
                        Ok(b"/continue".to_vec())
                    }
                }
            }
            _=>{
                //track_assert_eq!(Some(format!("continue {:?} handshake {:?}",ROW_INDEX.lock().unwrap(),channel.handshake)),None,ErrorKind::InvalidInput);
                Ok(b"/continue".to_vec())
            }
        }
        
    }
    pub fn process_handshake_res(&mut self,original_message:String)->Result<Vec<u8>>{ //bool: continue
        let mut consolidated:Vec<TcpItem> = vec![];
        match mem::replace(&mut self.handshake_res, HandshakeRes::Done) {
            HandshakeRes::RecvResponse(mut decoder)=>{
                // let mut decoder = ResponseDecoder::<BodyDecoder<RemainingBytesDecoder>>::default();
                let result = decoder.decode_from_read_buf(&mut self.ws_resbuf);
                if result.is_ok() && !decoder.is_idle() {
                    self.handshake_res = HandshakeRes::RecvResponse(decoder);
                    return Ok(b"/continue".to_vec());
                }
                match decoder.finish_decoding(){
                    Ok(response)=>{
                        let handshake_ok =  if response.status_code() == StatusCode::new(101)?{
                            String::from("handshake res ok")
                        }else{
                            format!("handshake res not ok, status code {:?}",response.status_code())
                        };
                        let mut consolidated:Vec<TcpItem> = vec![];
                        let item = TcpItem{
                            Payload:original_message,
                            String:handshake_ok,
                            Id:format!("{}-{}:Handshake res",self.laddr,self.raddr),
                            Laddr:self.laddr.clone(),
                            Raddr:self.raddr.clone()
                        };
                        consolidated.push(item);
                        let buf = rmp_serde::to_vec(&consolidated)?;
                        return Ok(buf);
                    }
                    _=>{}
                }
                
            }
            _=>{}
        }
        let item = TcpItem{
            Payload:original_message,
            String:format!("handshake "),
            Id:format!("{}-{}:Handshake res",self.laddr,self.raddr),
            Laddr:self.laddr.clone(),
            Raddr:self.raddr.clone()
        };
        consolidated.push(item);
        let buf = rmp_serde::to_vec(&consolidated)?;
        return Ok(buf);
    }
}
fn modify_request_origin(request:&Request<()>,origin:&str)->Request<()>{
    let mut new_request = Request::new(request.method() ,request.request_target(),request.http_version(),());
    unsafe{
        for field in request.header().fields() {
            let name = field.name();
            let value = field.value();
            if name=="origin"|| name=="Origin"{
                new_request.header_mut().add_field(HeaderField::new_unchecked(name,origin));
            }else{
                new_request.header_mut().add_field(HeaderField::new_unchecked(name,value));
            }
        }
    }
    new_request
}
