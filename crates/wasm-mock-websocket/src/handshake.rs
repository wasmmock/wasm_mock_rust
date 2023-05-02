use httpcodec::{RequestDecoder,BodyDecoder,ResponseDecoder,NoBodyDecoder};
use bytecodec::bytes::RemainingBytesDecoder;
use bytecodec::Decode;
pub enum Handshake {
    RecvRequest(RequestDecoder<NoBodyDecoder>),
    Done,
}
impl std::fmt::Debug for Handshake {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self{
            Handshake::RecvRequest(d)=>{
                write!(f, "Handshake RecvRequest {:?} {{ .. }}",d.is_idle())
            }
            Handshake::Done=>{
                write!(f, "Handshake Done {{ .. }}")
            }
        }
        
    }
}
pub enum HandshakeRes {
    RecvResponse(ResponseDecoder<BodyDecoder<RemainingBytesDecoder>>),
    Done,
}