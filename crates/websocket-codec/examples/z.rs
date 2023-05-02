use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::{i64, io, result};
use base64::{encode,decode};
use bytes::{Buf, BytesMut};
use tokio_util::codec::Decoder;
use websocket_codec::protocol::{DataLength, FrameHeader, FrameHeaderCodec};
use websocket_codec::{Opcode, Result,MessageCodec};
use httpcodec::{
    HeaderField, HttpVersion, NoBodyDecoder, NoBodyEncoder, ReasonPhrase, Request, RequestDecoder,RequestEncoder,
    Response, ResponseEncoder, StatusCode,BodyEncode
};
use bytecodec::{Decode,Encode};
use bytecodec::io::{IoDecodeExt, IoEncodeExt, ReadBuf, StreamState, WriteBuf};
use std::io::Cursor;
use std::io::{Write};
const BUF_SIZE: usize = 4096;
fn modify_request_origin(request:&Request<()>,origin:&str)->Request<()>{
    let mut new_request = Request::new(request.method() ,request.request_target(),request.http_version(),());
    unsafe{
        for field in request.header().fields() {
            let name = field.name();
            let value = field.value();
            println!("name {:?} {:?}",name,value);
            if name=="origin"|| name=="Origin"{
                new_request.header_mut().add_field(HeaderField::new_unchecked(name,origin));
            }else{
                new_request.header_mut().add_field(HeaderField::new_unchecked(name,value));
            }
        }
    }
    new_request

}
fn main() {
    //let buf = [129, 136, 187, 233, 66, 87, 212, 159, 39, 37, 130, 217, 114, 103];
    let mut buf = BytesMut::with_capacity(0);
    buf.extend_from_slice(&[129, 136, 187, 233, 66, 87, 212, 159, 39, 37, 130, 217, 114,103]);
    //buf.extend_from_slice(&[129, 136, 187, 233, 66, 87, 212, 159, 39, 37, 130, 217, 114, 103]); //full
    let s = String::from_utf8_lossy(&buf);
    println!("s {:?}",s);
    //let mut read_buf = BytesMut::from(&buf);
    // //let prev_remaining = read_buf.remaining();
    let mut h = MessageCodec::client();
    let mut z = FrameHeaderCodec{};
    //println!("z {:?}",z.decode(&mut buf));

    let result = h.decode(&mut buf);
    println!("result {:?} buf len{:?} frame_length{:?}",result,buf.len(),h.frame_length);
    println!("buf {:?}",std::str::from_utf8(&buf.to_vec()).unwrap());
    let mut rd : RequestDecoder<NoBodyDecoder> = RequestDecoder::default();
    let mut s = String::from("R0VUIC9lY2hvIEhUVFAvMS4xDQpIb3N0OiBsb2NhbGhvc3Q6MzMzNQ0KUHJhZ21hOiBuby1jYWNoZQ0KQWNjZXB0OiAqLyoNClNlYy1XZWJTb2NrZXQtS2V5OiBzUGVROFFaYU0rdkNQZjlZVHE4dU9nPT0NClNlYy1XZWJTb2NrZXQtVmVyc2lvbjogMTMNCkFjY2VwdC1MYW5ndWFnZTogZW4tU0csZW4tR0I7cT0wLjksZW47cT0wLjgNClNlYy1XZWJTb2NrZXQtRXh0ZW5zaW9uczogcGVybWVzc2FnZS1kZWZsYXRlDQpDYWNoZS1Db250cm9sOiBuby1jYWNoZQ0KQWNjZXB0LUVuY29kaW5nOiBnemlwLCBkZWZsYXRlDQpPcmlnaW46IGh0dHA6Ly9sb2NhbGhvc3Q6MzMzNA0KVXNlci1BZ2VudDogTW96aWxsYS81LjAgKE1hY2ludG9zaDsgSW50ZWwgTWFjIE9TIFggMTBfMTVfNykgQXBwbGVXZWJLaXQvNjA1LjEuMTUgKEtIVE1MLCBsaWtlIEdlY2tvKSBWZXJzaW9uLzE2LjEgU2FmYXJpLzYwNS4xLjE1DQpDb25uZWN0aW9uOiBVcGdyYWRlDQpVcGdyYWRlOiB3ZWJzb2NrZXQNCg0K");
    let msg = decode(s).unwrap();
    let mut ws_rbuf:ReadBuf<Vec<u8>> = ReadBuf::new(vec![0;BUF_SIZE]);
    // let mut chunks = msg.chunks(400);
    // let mut file = Cursor::new(chunks.next().unwrap());
    let mut file = Cursor::new(msg);
    ws_rbuf.fill(&mut file);
    let result = rd.decode_from_read_buf(&mut ws_rbuf);
    //if !rd.is_idle() {
       println!("is_idle {:?} rb{:?} ws_rbuflen{:?}",rd.is_idle(),rd.requiring_bytes(),ws_rbuf.len());
    //}
    match result.and_then(|()| rd.finish_decoding()) {
        Err(e) => {
            println!("e {:?}",rd);
            // self.handshake = Handshake::response_bad_request();
            // return Ok(false);
        }
        Ok(mut request) => {
            let new_req = modify_request_origin(&request,"http://localhost:3335");
            //println!("new_req{:?}",new_req);
            let mut encoder:RequestEncoder<NoBodyEncoder>= RequestEncoder::default();
            encoder.start_encoding(new_req).unwrap();
            let mut buf = Vec::new();
            encoder.encode_all(&mut buf).unwrap();
            let mut file = Cursor::new(buf);
            ws_rbuf.fill(&mut file);
        }
    }
}