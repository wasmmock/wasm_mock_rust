extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
use wasm_mock_websocket::*;
use wasm_mock_util::*;
use httparse;
fn command(msg: &[u8]) -> CallResult{
    let command = String::from("http://localhost:8000/t.json");
    Ok(command.as_bytes().to_vec())
}
fn request(msg: &[u8]) -> CallResult{
    //let mut headers = [httparse::EMPTY_HEADER; 10];
    let mut headers = [httparse::Header{name:"mime-type",value:b"application/json"}];
    let mut req = httparse::Request::new(&mut headers);
    req.path = Some("/j.json");
    req.method = Some("GET");
    let http1x = request_to_http1x(&req);
    let http_req = HttpRequest{
        Http1x:http1x,
        HttpBody:vec![],
        ProxyUrl:String::from(""),
    };
    let request = serde_json::to_string(&http_req)?;
    Ok(request.as_bytes().to_vec())
}

#[no_mangle]
pub extern "C" fn _start() {
    REGISTRY.lock().unwrap().insert("command".into(),command);
    REGISTRY.lock().unwrap().insert("request".into(),request);
    //REGISTRY.lock().unwrap().insert("response_marshalling".into(),response_marshalling);
}
fn main(){
 
}