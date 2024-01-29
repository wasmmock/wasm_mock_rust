extern crate wapc_guest as guest;
use guest::prelude::*;
pub static mut UID: Vec<u8> = vec![];
pub static mut WS_UID: Vec<u8> = vec![];
/// Used for HTTP Path during automation
pub static mut COMMAND: Vec<u8> = vec![];
//pub static mut AT_COUNTER: i32 = 0;
use lazy_static::lazy_static;
use std::sync::{Arc,Mutex};
use std::vec;
use byteorder::{ByteOrder, LittleEndian};
pub use byteorder;
lazy_static!{
    /// HashMap for storing WAPC HandlerSignatures. These will handler signatures will be registered when the host calls save_uid 
    pub static ref REGISTRY: Arc<Mutex<HashMap<String,fn(&[u8]) -> CallResult>>> = Arc::new(Mutex::new(HashMap::new()));
    pub static ref AT_COUNTER:Arc<Mutex<HashMap<String,u64>>> = Arc::new(Mutex::new(HashMap::new()));
    pub static ref AT_COUNTER2:Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    pub static ref COMMAND_MAP: Arc<Mutex<HashMap<i32,String>>> = Arc::new(Mutex::new(HashMap::new()));
    pub static ref REQUEST_MAP: Arc<Mutex<HashMap<i32,HttpRequest>>> = Arc::new(Mutex::new(HashMap::new()));
    pub static ref REQUEST_MAR_MAP: Arc<Mutex<HashMap<i32,String>>> = Arc::new(Mutex::new(HashMap::new()));
    pub static ref RESPONSE_MAR_MAP: Arc<Mutex<HashMap<i32,fn(msg: &[u8])->CallResult>>> = Arc::new(Mutex::new(HashMap::new()));
    pub static ref HOST_MAP: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
}
#[no_mangle]
pub extern "C" fn wapc_init() {
    register_function("save_uid", save_uid);
    register_function("save_ws_uid", save_ws_uid);
    register_function("save_command",save_command);
    register_function("get_uid", get_uid);
    register_function("version", version);
    // register_function("request_marshalling",do_nothing);
    // register_function("response_marshalling",do_nothing);
    register_function("add_functions",add_functions);
    register_function("add_ws_functions",add_ws_functions);
    register_function("command", |msg:&[u8]|->CallResult{
        let index = foo_index() as i32;
        if let Some(i)= COMMAND_MAP.lock().unwrap().get(&index){
            return Ok(i.clone().into_bytes())
        }
        Ok(vec![])
    });
    register_function("request", |msg:&[u8]|->CallResult{
        let index = foo_index() as i32 ;
        if let Some(i)= REQUEST_MAP.lock().unwrap().get(&index){
            let request = serde_json::to_string(&i).unwrap();
            return Ok(request.into_bytes())
        }
        Ok(vec![])
    });
    register_function("request_marshalling", |msg:&[u8]|->CallResult{
        let index = foo_index() as i32;
        if let Some(i)= REQUEST_MAR_MAP.lock().unwrap().get(&index){
            return Ok(i.clone().into_bytes())
        }
        Ok(vec![])
    });
    register_function("response_marshalling", |msg:&[u8]|->CallResult{
        let index = foo_index() as i32;
        if let Some(i)= RESPONSE_MAR_MAP.lock().unwrap().get(&index){
            return i(msg.clone());
        }
        return Ok(msg.to_vec())
    });
}
fn do_nothing(msg:&[u8]) -> CallResult{
    Ok(msg.to_vec())
}
fn add_functions(_:&[u8]) -> CallResult{
    let mut s = String::from("");
    for (k,v) in REGISTRY.lock().unwrap().iter(){
        let k_c = k.clone();
        register_function(&k_c,v.clone());
        let k_c2 = k.clone();
        s.push_str(&k_c2);
        s.push_str(",");
    }
    
    register_function("loop",|_|->CallResult{

        if let Ok(ac) = AT_COUNTER2.lock(){
            // let mut buf = [0; 8];
            // let n = *ac;
            // let n_c = n.clone();
            // LittleEndian::write_u64(&mut buf,n_c as u64);
            // let d = format!("{:?}",buf.to_vec());
            // //return Ok(d.into_bytes());
            // return Ok(vec![1,0,0,0,0,0,0,0]);
            let n = *ac;

            let d = format!("_{}",n.clone().to_string());
            return Ok(d.into_bytes());
        }
      
        //Ok(buf.clone().to_vec())
        // let z = format!("_{:?}",*ac);
        Ok(vec![])
    });
    Ok(s.as_bytes().to_vec())
}
fn version(_msg: &[u8]) -> CallResult {
    let s = String::from("0.1.0");
    Ok(s.as_bytes().to_vec())
}
fn get_uid(_msg: &[u8]) -> CallResult {
    unsafe { Ok(UID.clone()) }
}
fn save_uid(msg: &[u8]) -> CallResult {
    unsafe {
        UID = msg.to_vec();
    }
    
    Ok(b"".to_vec())
}
fn save_ws_uid(msg: &[u8]) -> CallResult {
    unsafe {
        WS_UID = msg.to_vec();
    }
    Ok(b"".to_vec())
}
fn save_command(msg: &[u8]) -> CallResult {
    unsafe {
        COMMAND = msg.to_vec();
    }
    Ok(b"".to_vec())
}
fn add_ws_functions(msg: &[u8]) -> CallResult{
    let mock_targets = std::str::from_utf8(msg).unwrap();
    let mock_targets_list:Vec<&str> = mock_targets.split(",").collect();
    for name in mock_targets_list.iter(){
      register_function(&name, |m: &[u8]|->CallResult{
        //foo_websocket!(m)
        foo_websocket(m)
      });
      let name_req_json = format!("{}_req_json",name); 
      register_function(&name_req_json, |m: &[u8]|->CallResult{
        foo_websocket_req_json(m)
      });
      let name_res_json = format!("{}_res_json",name); 
      register_function(&name_res_json, |m: &[u8]|->CallResult{
        foo_websocket_res_json(m)
      }); 
    }
    
    Ok(msg.to_vec())
}
/// Get index. Index increments per iteration of the loop defined in automation
pub fn foo_index()->i64{
    
    let mut _uid = "";
    unsafe {
        _uid = std::str::from_utf8(&UID).unwrap();
    }
    let s = host_call(_uid, "foo", "get_index", b"").unwrap();
    LittleEndian::read_u64(&s) as i64
}
/// Assert equal macro
///
/// # Examples
///
/// ```

/// ```
///
/// # Arguments
///
/// * `left` - Left
/// * `right` - Right
/// * `desc` - Name of this assertion test
#[macro_export]
macro_rules! foo_assert_eq {
    ($left:expr, $right:expr, $desc:expr) => {{
        match (&$left, &$right) {
            (left_val, right_val) => {
                let mut uid = "";
                unsafe {
                    uid = std::str::from_utf8(&UID)?;
                }
                if !(*left_val == *right_val) {
                    let s = format!(
                        "`{}` assertion failed: `(left == right)`
              left: `{:?}`, right: `{:?}",
                        $desc, &*left_val, &*right_val
                    );
                    host_call(uid, "foo", "assert_fail", s.as_bytes()).unwrap();
                } else {
                    let s = format!("`{}` assertion passed: `{:?}`", $desc, &*left_val);
                    host_call(uid, "foo", "assert_pass", s.as_bytes()).unwrap();
                }
            }
        }
    }};
}
/// Assert Equal Macro that will only be executed if an indexdbkey is toggled true
#[macro_export]
macro_rules! foo_assert_eq_toggle {
    ($left:expr, $right:expr, $desc:expr,$indexdbkey:expr) => {{
        if foo_indexdb_get!($indexdbkey, "nil") == "true" {
            match (&$left, &$right) {
                (left_val, right_val) => {
                    let mut uid = "";
                    unsafe {
                        uid = std::str::from_utf8(&UID)?;
                    }
                    if !(*left_val == *right_val) {
                        let s = format!(
                            "`{}` assertion failed: `(left == right)`
              left: `{:?}`, right: `{:?}",
                            $desc, &*left_val, &*right_val
                        );
                        host_call(uid, "foo", "assert_fail", s.as_bytes()).unwrap();
                    } else {
                        let s = format!("`{}` assertion passed: `{:?}`", $desc, &*left_val);
                        host_call(uid, "foo", "assert_pass", s.as_bytes()).unwrap();
                    }
                }
            }
        }
    }};
}
/// Assert true/false macro
///
/// # Arguments
///
/// * `cond` - true/false
/// * `desc` - Name of the assertion
#[macro_export]
macro_rules! foo_assert {
    ($cond:expr,$desc:expr) => {{
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&UID)?;
        }
        if $cond {
            host_call(uid, "foo", "assert_pass", $desc.as_bytes()).unwrap();
        } else {
            host_call(uid, "foo", "assert_fail", $desc.as_bytes()).unwrap();
        }
    }};
}
/// Automation step validation if the step passes or fails. It is usually used by other macros.
///
/// # Arguments
///
/// * `cond` - true/false
/// * `desc` - Name of the step operation
#[macro_export]
macro_rules! foo_step {
    ($cond:expr,$desc:expr) => {{
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&UID)?;
        }
        if $cond {
            host_call(uid, "foo", "step_pass", $desc.as_bytes()).unwrap();
        } else {
            host_call(uid, "foo", "step_fail", $desc.as_bytes()).unwrap();
        }
    }};
}
/// Embedded version code used by wasm-mock-util
#[macro_export]
macro_rules! foo_version {
    () => {
        fn version(_msg: &[u8]) -> CallResult {
            let s = String::from("0.1.0");
            Ok(s.as_bytes().to_vec())
        }
    };
}
/// Save UID into static mut UID
#[macro_export]
macro_rules! foo_save_uid {
    () => {
        fn save_uid(msg: &[u8]) -> CallResult {
            unsafe {
                UID = msg.to_vec();
            }
            Ok(b"".to_vec())
        }
    };
}
/// Only used to construct the dynamic websocket wasm 
#[macro_export]
macro_rules! foo_save_ws_uid {
    () => {
        fn save_ws_uid(msg: &[u8]) -> CallResult {
            unsafe {
                WS_UID = msg.to_vec();
            }
            Ok(b"".to_vec())
        }
    };
}
/// Macro that hides function to save command string into the wasm (which refers to http api route)
#[macro_export]
macro_rules! foo_save_command {
    () => {
        fn save_command(msg: &[u8]) -> CallResult {
            unsafe {
                COMMAND = msg.to_vec();
            }
            Ok(b"".to_vec())
        }
    };
}
/// Marco that hides function to register get_uid function
#[macro_export]
macro_rules! foo_get_uid {
    () => {
        fn get_uid(_msg: &[u8]) -> CallResult {
            unsafe { Ok(UID.clone()) }
        }
    };
}
/// Macro that does host call "rpc_request", only used in conjunction of deliberate config inside wasm mock server
#[macro_export]
macro_rules! foo_rpc_request {
    ($command:expr,$request:expr,$type:ty) => {{
        let reqjson = serde_json::to_string(&$request.clone())?;
        let mut payload = vec![];
        $request.encode(&mut payload)?;
        match host_call($command, "foo", "rpc_request", &*payload) {
            Ok(res) => {
                let m = <$type>::decode(&*res)?;
                let j = serde_json::to_string(&m)?;
                foo_step!(
                    true,
                    format!("RPC cmd:{} req:{} res:{}", $command, reqjson, j)
                );
                Ok(m)
            }
            Err(e) => {
                foo_step!(
                    false,
                    format!("RPC cmd:{} req:{} res:{}", $command, reqjson, e)
                );
                Err(e)
            }
        }
    }};
}
/// Macro that does external HTTP host call and returns json HTTP response body. Usually used as oracle of truth during assertion.
///
/// # Arguments
///
/// * `addr` - true/false
/// * `request` - Http1x string
/// * `body` - Body in bytes
/// * `proxy_url` - HTTP Proxy URL
#[macro_export]
macro_rules! foo_http_request {
    ($addr:expr,$request:expr,$body:expr,$proxy_url:expr) => {{
        let r = HttpRequest {
            Http1x: $request.to_string(),
            HttpBody: $body.as_bytes().to_vec(),
            ProxyUrl: $proxy_url.to_string(),
        };
        let request = serde_json::to_string(&r)?;
        match host_call($addr, "foo", "http_request", request.as_bytes()) {
            Ok(res) => {
                let j = std::str::from_utf8(&res)?;
                foo_step!(true, format!("HTTP req:{} res:{}", request, j));
                match serde_json::from_str(j) {
                    Ok(res) => Ok(res),
                    Err(e) => {
                        let io_error: std::io::Error = e.into();
                        let err_ref = io_error.into_inner().unwrap();
                        Err(err_ref)
                    }
                }
            }
            Err(e) => {
                foo_step!(false, format!("HTTP req:{} err:{}", request, e));
                Err(e)
            }
        }
    }};
}
#[macro_export]
macro_rules! foo_http_requestOld {
    ($addr:expr,$request:expr,$body:expr,$proxy_url:expr) => {{
        let r = HttpRequest {
            Http1x: $request.to_string(),
            HttpBody: $body.as_bytes().to_vec(),
            ProxyUrl: $proxy_url.to_string(),
        };
        let request = serde_json::to_string(&r)?;
        match host_call($addr, "foo", "http_request", request.as_bytes()) {
            Ok(res) => {
                let j = std::str::from_utf8(&res)?;
                foo_step!(true, format!("HTTP req:{} res:{}", request, j));
                match serde_json::from_str(j) {
                    Ok(res) => Ok(res),
                    Err(e) => {
                        let io_error: std::io::Error = e.into();
                        let err_ref = io_error.into_inner().unwrap();
                        Err(err_ref)
                    }
                }
            }
            Err(e) => {
                foo_step!(false, format!("HTTP req:{} err:{}", request, e));
                Err(e)
            }
        }
    }};
}
/// Macro that sends a tcp payload from mock server to remote connection
/// # Arguments
///
/// * `addr` - {local address}-:{remote address}
/// * `request` - Tcp payload
#[macro_export]
macro_rules! foo_tcp_request {
    ($addr:expr,$request:expr) => {{
        use std::boxed::Box;
        let mut buf = vec![];
        $request.serialize(&mut Serializer::new(&mut buf))?;
        match host_call($addr, "foo", "tcp_request", &buf) {
            Ok(res) => {
                let tcp_res: TcpReq = rmp_serde::from_read_ref(&res)?;
                foo_step!(true, format!("TCP req:{} res:{}", $request.String, tcp_res.String));
                Ok(res)
            }
            Err(e) => {
                foo_step!(false, format!("TCP req:{} err:{}", $request.String, e));
                Err(e)
            }
        }
    }};
}
/// Macro that sends a tcp payload from mock server to local connection
/// # Arguments
///
/// * `addr` - {local address}-:{remote address}
/// * `request` - Tcp payload
#[macro_export]
macro_rules! foo_tcp_response {
    ($addr:expr,$request:expr) => {{
        use std::boxed::Box;
        let mut buf = vec![];
        $request.serialize(&mut Serializer::new(&mut buf))?;
        match host_call($addr, "foo", "tcp_response", &buf) {
            Ok(res) => {
                let tcp_res: TcpReq = rmp_serde::from_read_ref(&res)?;
                foo_step!(true, format!("TCP res:{} res:{}", $request.String, tcp_res.String));
                Ok(res)
            }
            Err(e) => {
                foo_step!(false, format!("TCP req:{} err:{}", $request.String, e));
                Err(e)
            }
        }
    }};
}
/// Macro that does simple redis command
/// # Arguments
///
/// * `addr` - redis address
/// * `method` - get/delete
/// * `key` - cache key
#[macro_export]
macro_rules! foo_redis {
    ($addr:expr,$method:expr,$key:expr) => {{
        match host_call(
            $addr,
            "foo",
            &*format!("redis_{}", $method),
            $key.as_bytes(),
        ) {
            Ok(res) => {
                let j = std::str::from_utf8(&res)?;
                if $method == "delete" {
                    foo_step!(true, format!("REDIS {} {} {}", &$method, $key, j));
                } else {
                    foo_assert!(true, format!("REDIS {} {} {}", &$method, $key, j));
                }
                Ok(res)
            }
            Err(e) => {
                if $method == "delete" {
                    foo_step!(false, format!("REDIS {} {} {}", &$method, $key, e));
                } else {
                    foo_assert!(false, format!("REDIS {} {} {}", &$method, $key, e));
                }
                Err(e)
            }
        }
    }};
}
/// Macro that does simple memcache command
/// # Arguments
///
/// * `addr` - memcache address
/// * `method` - get/delete
/// * `key` - cache key
#[macro_export]
macro_rules! foo_memcache {
    ($method:expr,$addr:expr,$key:expr) => {{
        match host_call(
            $addr,
            "foo",
            &*format!("memcache_{}", $method),
            $key.as_bytes(),
        ) {
            Ok(res) => {
                let j = std::str::from_utf8(&res)?;
                if $method == "delete" {
                    foo_step!(true, format!("MEMCACHE {} {} {}", &$method, $key, j));
                } else {
                    foo_assert!(true, format!("MEMCACHE {} {} {}", &$method, $key, j));
                }
                Ok(res)
            }
            Err(e) => {
                if $method == "delete" {
                    foo_step!(false, format!("MEMCACHE {} {} {}", &$method, $key, e));
                } else {
                    foo_assert!(false, format!("MEMCACHE {} {} {}", &$method, $key, e));
                }
                Err(e)
            }
        }
    }};
}
/// Macro that stores key & value inside mock server. Can also done in HTTP POST /indexdb/store {"key":"a","value":"a"}
/// # Arguments
///
/// * `key` - key
/// * `value` - value
#[macro_export]
macro_rules! foo_indexdb_store {
    ($key:expr,$value:expr) => {{
        match host_call($key, "foo", "indexdb_store", $value.as_bytes()) {
            Ok(res) => Ok(res),
            Err(e) => {
                foo_step!(false, format!("IndexDbStore {} {}", $key, e));
                Err(e)
            }
        }
    }};
}
/// Macro that get key & value inside mock server. Can also done in HTTP GET /indexdb/get?key=a
/// # Arguments
///
/// * `key` - key
/// * `default` - default if operation fails
///
/// # Returns
///
/// Indexdb value or default value in bytes
#[macro_export]
macro_rules! foo_indexdb_get {
    ($key:expr,$default:expr) => {{
        match host_call($key, "foo", "indexdb_get", $default.as_bytes()) {
            Ok(res) => {
                let s = std::str::from_utf8(&res).unwrap().to_owned();
                foo_step!(true, format!("IndexDbGet {} {}", $key, s));
                s
            }
            Err(e) => {
                foo_step!(false, format!("IndexDbGet {} default {}", $key, $default));
                $default.to_owned()
            }
        }
    }};
}
/// Macro that get key & value inside mock server. Can also done in HTTP GET /indexdb/get?key=a
/// # Arguments
///
/// * `key` - key
/// * `default` - default if operation fails
///
/// # Returns
///
/// Indexdb value or default value in bytes
#[macro_export]
macro_rules! foo_mysql {
    ($key:expr,$default:expr) => {{
        match host_call($key, "foo", "mysql", $default.as_bytes()) {
            Ok(res) => {
                let s = std::str::from_utf8(&res).unwrap().to_owned();
                foo_step!(true, format!("mysql {} {}", $key, s));
                s
            }
            Err(e) => {
                foo_step!(false, format!("mysql {} default {}", $key, $default));
                $default.to_owned()
            }
        }
    }};
}
/// Automation runs in a loop with an index that starts from 0. This macro gets the index.
#[macro_export]
macro_rules! foo_index {
    () => {{
        use byteorder::{ByteOrder, LittleEndian};
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&UID).unwrap();
        }
        let s = host_call(uid, "foo", "get_index", b"")?;
        LittleEndian::read_u64(&s) as i64
    }};
}
/// Macro that sleep for duration in milliseconds
#[macro_export]
macro_rules! foo_sleep {
    ($key:expr) => {{
        use byteorder::{ByteOrder, LittleEndian};
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&UID).unwrap();
        }
        let mut buf = [0; 8];
        LittleEndian::write_u64(&mut buf, $key);
        match host_call(uid, "foo", "sleep", &buf) {
            Ok(res) => {
                foo_step!(true, format!("Sleep {} milliseconds", $key));
                Ok(res)
            }
            Err(e) => {
                foo_step!(false, format!("Sleep {} milliseconds, error: {}", $key, e));
                Err(e)
            }
        }
    }};
}
/// Macro that save bytes into a file inside mock server
///
/// # Arguments
///
/// * `payload` - bytes to be saved in bytes array
/// * `path` - file destination
#[macro_export]
macro_rules! foo_save_file {
    ($payload:expr,$path:expr) => {{
        match host_call($path, "foo", "savefile", $payload) {
            Ok(res) => {
                Ok(res)
            }
            Err(e) => {
                Err(e)
            }
        }
    }};
}
/// Only used to construct the dynamic websocket wasm
#[macro_export]
macro_rules! foo_websocket {
    ($payload:expr) => {{
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&WS_UID).unwrap();
        }
        let mut fun = "";
        unsafe {
            fun = std::str::from_utf8(&COMMAND).unwrap();
        }
        let binding = format!("{}|{}", uid, fun);
        host_call(&binding, "foo", "websocket", $payload)
    }};
}
fn foo_websocket(payload:&[u8])->CallResult{
    let mut _uid = "";
    unsafe {
        _uid = std::str::from_utf8(&WS_UID).unwrap();
    }
    let mut _fun = "";
    unsafe {
        _fun = std::str::from_utf8(&COMMAND).unwrap();
    }
    let binding = format!("{}|{}", _uid, _fun);
    host_call(&binding, "foo", "websocket", payload)
}
/// Only used to construct the dynamic websocket wasm
#[macro_export]
macro_rules! foo_websocket_req_json {
    ($payload:expr) => {{
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&WS_UID).unwrap();
        }
        let mut fun = "";
        unsafe {
            fun = std::str::from_utf8(&COMMAND).unwrap();
        }
        let binding = format!("{}|{}_req_json", uid, fun);
        host_call(&binding, "foo", "websocket", $payload)
    }};
}
fn foo_websocket_req_json(payload:&[u8])->CallResult{
    let mut _uid = "";
    unsafe {
        _uid = std::str::from_utf8(&WS_UID).unwrap();
    }
    let mut _fun = "";
    unsafe {
        _fun = std::str::from_utf8(&COMMAND).unwrap();
    }
    let binding = format!("{}|{}_req_json", _uid, _fun);
    host_call(&binding, "foo", "websocket", payload)
}
fn foo_websocket_res_json(payload:&[u8])->CallResult{
    let mut _uid = "";
    unsafe {
        _uid = std::str::from_utf8(&WS_UID).unwrap();
    }
    let mut _fun = "";
    unsafe {
        _fun = std::str::from_utf8(&COMMAND).unwrap();
    }
    let binding = format!("{}|{}_res_json", _uid, _fun);
    host_call(&binding, "foo", "websocket", payload)
}
/// Only used to construct the dynamic websocket wasm
#[macro_export]
macro_rules! foo_websocket_res_json {
    ($payload:expr) => {{
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&WS_UID).unwrap();
        }
        let mut fun = "";
        unsafe {
            fun = std::str::from_utf8(&COMMAND).unwrap();
        }
        let binding = format!("{}|{}_res_json", uid, fun);
        host_call(&binding, "foo", "websocket", $payload)
    }};
}
/// Only used to construct the dynamic websocket wasm
#[macro_export]
macro_rules! foo_websocket_call {
    ($key:expr,$payload:expr) => {{
        let mut uid = "";
        unsafe {
            uid = std::str::from_utf8(&WS_UID).unwrap();
        }
        let index = foo_index!();
        let binding = format!("{}|{}|{}", uid, $key, index);
        host_call(&binding, "foo", "websocket_call", $payload)
    }};
}
/// Macro that compares between two http_headers
#[macro_export]
macro_rules! foo_compare_http_header {
    ($a:expr,$b:expr) => {{
      for (key,val) in $a{
        if let Some(val_b)= $b.get(key){
          match assert_json_matches_no_panic(val,val_b,Config::new(CompareMode::Inclusive)){
            Ok(())=>{}
            Err(err)=>{
              let mut arrange = false;
              let q:Vec<&str> = err.split("\"").collect();
              // if q.len() >=3{
              //   let quote = q[1];
              //   if quote.contains("["){
              //     arrange = true;
              //     let path_array = quote.split(".");
              //     let mut v_a = val;
              //     let mut v_b = val_b;
              //     for p in path_array{
              //       v_a = v_a.get(p).unwrap();
              //       v_b = v_b.get(p).unwrap();
              //     }
              //     //if let serde_json::value::Value::Array(_)= 
              //   }
              // }
              
              if !arrange{
                foo_assert!(false,format!("HTTP header key:{}, {}",key,err));
              }
            }
          }
        }else{
          foo_assert!(false,format!("cannot find HTTP header in rhs: {}",key));
        }
      }
    }};
}
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use httparse;
// use assert_json_diff::{assert_json_matches_no_panic,Config,CompareMode};
pub fn mock_register_function<
    Req: prost::Message + Serialize + Default,
    Res: prost::Message + Serialize + Default,
>(
    fn_str: &str,
    func: fn(&[u8]) -> CallResult,
) {
    register_function(fn_str, func);
    register_function(&format!("{}_res_json", &fn_str), |msg| {
        let m = <Res>::decode(msg)?;
        let j = serde_json::to_string(&m)?;
        Ok(j.as_bytes().to_vec())
    });
    register_function(&format!("{}_req_json", &fn_str), |msg| {
        let m = <Req>::decode(msg)?;
        let j = serde_json::to_string(&m)?;
        Ok(j.as_bytes().to_vec())
    });
}

pub fn mock_register_function_tff(
    fn_str: &str,
    func: fn(&[u8]) -> CallResult,
) {
    register_function(fn_str, func);
    register_function(&format!("{}_res_json", &fn_str), |msg| {
        Ok(msg.to_vec())
    });
    register_function(&format!("{}_req_json", &fn_str), |msg| {
        Ok(msg.to_vec())
    });
}
/// Get current time stamp
pub fn now() -> Result<i64, Box<dyn Error + Sync + Send>> {
    let now_payload = host_call("default", "foo", "now", b"")?;
    Ok(LittleEndian::read_u64(&now_payload) as i64)
}
// /// Get index by uid
// pub fn foo_index(uid: &str) -> Result<i64, Box<dyn Error + Sync + Send>> {
//     let s = host_call(uid, "foo", "get_index", b"")?;
//     Ok(LittleEndian::read_u64(&s) as i64)
// }
/// Function used in HTTP "res_json"
pub fn foo_http_response(msg: &[u8]) -> Result<HttpResponse, Box<dyn Error + Sync + Send>> {
    let j = std::str::from_utf8(&msg)?;
    match serde_json::from_str(j) {
        Ok(res) => Ok(res),
        Err(e) => {
            let io_error: std::io::Error = e.into();
            let err_ref = io_error.into_inner().unwrap();
            Err(err_ref)
        }
    }
}
/// Convenient way to unmarshall JSON into a type. Commonly used in "req_json"
pub fn foo_unmarshall2<T>(msg:&[u8]) ->Result<T, Box<dyn Error + Sync + Send>> where T:serde::de::DeserializeOwned{
  let j = std::str::from_utf8(&msg)?;
  match serde_json::from_str(j) {
      Ok(res) => Ok(res),
      Err(e) => {
          let io_error: std::io::Error = e.into();
          let err_ref = io_error.into_inner().unwrap();
          Err(err_ref)
      }
  }
}
/// Convenient way to unmarshall JSON into a type. Commonly used in "req_json"
pub fn foo_unmarshall<T>(msg:&[u8]) ->Result<T, Box<dyn Error + Sync + Send>> where T:serde::de::DeserializeOwned{
    match serde_json::from_slice(msg) {
        Ok(res) => Ok(res),
        Err(e) => {
            let io_error: std::io::Error = e.into();
            let err_ref = io_error.into_inner().unwrap();
            Err(err_ref)
        }
    }
  }
/// Function used in ".._fiddler_ab" to unmarshall into a type that is used for AB comparison testing
pub fn foo_fiddler_ab(msg: &[u8]) -> Result<FiddlerAB, Box<dyn Error + Sync + Send>> {
  let j = std::str::from_utf8(&msg)?;
  match serde_json::from_str(j) {
      Ok(res) => Ok(res),
      Err(e) => {
          let io_error: std::io::Error = e.into();
          let err_ref = io_error.into_inner().unwrap();
          Err(err_ref)
      }
  }
}
/// To be used in conjunction of foo_unmarshall in guest call "response" for HTTP automation
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug,Default,Clone)]
pub struct HttpResponse {
    #[serde(rename = "http_header")]
    pub HttpHeader: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "http_cookie")]
    pub HttpCookie: Option<HashMap<String, String>>,
    #[serde(rename = "http_body")]
    pub HttpBody: serde_json::Value,
    #[serde(rename = "http_body_raw")]
    pub HttpBodyRaw: String,
    #[serde(rename = "status_code")]
    pub StatusCode: String,
    #[serde(rename = "error")]
    pub Error: String,
    #[serde(rename = "http_req")]
    pub HttpReq: RequestReceivedInMock,
}
/// Return type for guest call "request"
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct HttpRequest {
    #[serde(rename = "http1x")]
    pub Http1x: String,
    #[serde(rename = "http_body")]
    pub HttpBody: Vec<u8>,
    #[serde(rename = "proxy_url")]
    pub ProxyUrl: String,
}
/// Return type for guest call "request"
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct HttpRequestV2 {
    #[serde(rename = "url")]
    pub Url: String,
    #[serde(rename = "http1x")]
    pub Http1x: String,
    #[serde(rename = "http_body")]
    pub HttpBody: Vec<u8>,
    #[serde(rename = "proxy_url")]
    pub ProxyUrl: String,
}
/// Type that is used to unmarshall in ".._req_json"
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug,Default,Clone)]
pub struct RequestReceivedInMock {
    #[serde(rename = "http_param")]
    pub HttpParam: Option<HashMap<String,Vec<String>>>,
    #[serde(rename = "http_header")]
    pub HttpHeader: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "http_cookie")]
    pub HttpCookie: Option<HashMap<String,String>>,
    #[serde(rename = "http_body")]
    pub HttpBody: serde_json::Value,
    #[serde(rename = "http_body_raw")]
    pub HttpBodyRaw: String,
    #[serde(rename = "http_proxy_url")]
    pub HttpProxyUrl: String,
    #[serde(rename = "http_path")]
    pub HttpPath: String,
    #[serde(rename = "http_scheme")]
    pub HttpScheme: String,
    #[serde(rename = "http_method")]
    pub HttpMethod: String,
}
/// Type that is used for AB testing for http
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct FiddlerAB {
    #[serde(rename = "res_a")]
    pub ResA: HttpResponse,
    #[serde(rename = "res_b")]
    pub ResB: HttpResponse,
    #[serde(rename = "url_path")]
    pub UrlPath: String,
}
/// Data structure  mock server and remote connection
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct TcpReq {
    /// Base64 encoded string
    pub Payload: String,
    /// Human readable representation of the payload for reporting
    pub String: String,
    /// Row index for reporting
    pub Index: i64,
    /// If Id is not empty string, the reporting recognizes this TCP Request as RPC. Mock server expects a response with similar Id from remote connection
    pub Id: String,
    /// Reporting only records one instance of traffic for each command
    pub Command: String,
    pub ReportType: String,
    /// If Timeout is true, the reporting recognizes this TCP Request as RPC and report a timeout error if the mock server does not receive response with similar Id from remote connection after 5 seconds
    pub Timeout: bool,
    /// Local connection address
    pub Laddr:String,
    /// Remote connection address
    pub Raddr:String,
    
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
    pub Tcp_Items: Vec<TcpItem>,
}
/// Type for AB comparison for Tcp
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct TcpFiddlerAB {
    pub ResA: Vec<u8>,
    pub ResB: Vec<u8>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct MysqlResponse {
    #[serde(rename = "http_header")]
    pub Data: Vec<Vec<Vec<u8>>>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct MysqlRequest {
    #[serde(rename = "query")]
    pub Query: String,
    #[serde(rename = "execute")]
    pub Execute: String,
    #[serde(rename = "query_column")]
    pub QueryColumn: String,
}
/// Utility function of converting Http Request to HTTP 1x which is used in foo_http_request
pub fn request_to_http1x(r: &httparse::Request)->String{
  let mut owned_string: String = "".to_owned();
  if let Some(m) = r.method{
    owned_string.push_str(m);
    owned_string.push_str(" / HTTP/1.1\r\n");
  }
  for x in 0..r.headers.len(){
    let httparse::Header{name,value} = r.headers[x];
    if name!=""{
      owned_string.push_str(name);
      owned_string.push_str(": ");
      owned_string.push_str(std::str::from_utf8(value).unwrap());
      owned_string.push_str("\r\n");
    }
  }
  owned_string.push_str("\r\n");
  owned_string
}
/// Utility function of converting Http Headers to string
pub fn header_to_string(h: &httparse::Header)->String{
  let mut owned_string: String = "".to_owned();
  owned_string.push_str(std::str::from_utf8(h.value).unwrap());
  owned_string
}