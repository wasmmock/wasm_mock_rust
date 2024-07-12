#[macro_export(local_inner_macros)]
macro_rules! modify {
    ( @parameters | $body:block $test_case_failed:ident ) => { 
        $body 
    };

    ( @parameters , $($remainder:tt)+ ) => {
        modify!(@parameters $($remainder)*);
    };
    (  $(#[$attr:meta])* http_ignore $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REQUEST_IGNORE_MAP.lock().unwrap().push(String::from($name));
    };  
    ( $(#[$attr:meta])* http_req $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_http_modify_req",$name),|msg:&[u8]|->CallResult{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = foo_unmarshall::<RequestReceivedInMock>(msg)?;
            //let mut $param = foo_unmarshall::<$param_ty>(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            let request = serde_json::to_string(&$param)?;
            Ok(request.as_bytes().to_vec())
        });
    };
    ( $(#[$attr:meta])* http_res $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_http_modify_res",$name),|msg:&[u8]|->CallResult{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = foo_unmarshall::<HttpResponse>(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            let request = serde_json::to_string(&$param)?;
            Ok(request.as_bytes().to_vec())
        });
    };
    ( $(#[$attr:meta])* http_replayer $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_http_fiddler_ab",$name),|msg:&[u8]|->CallResult{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = foo_fiddler_ab(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            //let request = serde_json::to_string(&$param)?;
            Ok(Vec::new())
        });
    };
    ( $(#[$attr:meta])* tcp_req $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_modify_req",$name),|msg:&[u8]|{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = tcp_foo_unmarshall::<TcpPayload>(&msg)?;
            let b = modify!(@parameters | $($args_and_body)* test_case_failed); //should be modify_tcp!
            let b_c= b?.clone();
            let len = b_c.len();
            if len==0{
                return Ok(b"/continue".to_vec())
            }else{
                return tcp_foo_marshall(&b_c)
            }
            //Ok(Vec::new())
        });
    };
    ( $(#[$attr:meta])* tcp_res $name:literal | $param:tt| $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_modify_res",$name),|msg:&[u8]|{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = tcp_foo_unmarshall::<TcpPayload>(&msg)?;
            let b =modify!(@parameters | $($args_and_body)* test_case_failed);
            let b_c= b?.clone();
            let len = b_c.len();
            if len==0{
                return Ok(b"/continue".to_vec())
            }else{
                return tcp_foo_marshall(&b_c)
            }
        });
    };
    ( $(#[$attr:meta])* tcp_replayer $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_fiddler_ab",$name),|msg:&[u8]|->CallResult{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = tcp_foo_fiddler_ab(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            Ok(Vec::new())
        });
    };
    
}
#[macro_export(local_inner_macros)]
macro_rules! test {
    ( @parameters | $body:block $test_case_failed:ident ) => { 
        $body 
    };

    ( @parameters , $($remainder:tt)+ ) => {
        test!(@parameters $($remainder)*);
    };

    ( $(#[$attr:meta])* http_get $name:literal | ($headers:expr) | ($payload:expr) | $param:tt | $body:block ) => {
        if let Ok(mut AC) = AT_COUNTER2.lock(){
            let mut host = HOST_MAP.lock().unwrap().clone();
            host.push_str($name);
            COMMAND_MAP.lock().unwrap().insert(AC.clone(),host.clone());
            
            let mut headers = $headers;
            let mut req = httparse::Request::new(&mut headers);
            req.method = Some("GET");
            let http1x = request_to_http1x(&req);
            let r = HttpRequest{
                Http1x:http1x.clone(),
                HttpBody:$payload,
                ProxyUrl:String::from("")
            };
            REQUEST_MAP.lock().unwrap().insert(AC.clone(),r);
            REQUEST_MAR_MAP.lock().unwrap().insert(AC.clone(),http1x);
            RESPONSE_MAR_MAP.lock().unwrap().insert(AC.clone(),|msg:&[u8]|->CallResult{
                let $param: HttpResponse = foo_http_response(msg.clone())?;
                $body
                Ok(msg.to_vec())
            });
            *AC+=1;
        }
     
    };
    ( $(#[$attr:meta])* http_post $name:literal | ($headers:expr) | ($payload:expr)  | $param:tt | $body:block ) => {
        if let Ok(mut AC) = AT_COUNTER2.lock(){
            let mut host = HOST_MAP.lock().unwrap().clone();
            host.push_str($name);
            COMMAND_MAP.lock().unwrap().insert(AC.clone(),host.clone());
            
            let mut headers = $headers;
            let mut req = httparse::Request::new(&mut headers);
            req.method = Some("POST");
            let http1x = request_to_http1x(&req);
            let r = HttpRequest{
                Http1x:http1x.clone(),
                HttpBody:$payload,
                ProxyUrl:String::from("")
            };
            REQUEST_MAP.lock().unwrap().insert(AC.clone(),r);
            REQUEST_MAR_MAP.lock().unwrap().insert(AC.clone(),http1x);
            RESPONSE_MAR_MAP.lock().unwrap().insert(AC.clone(),|msg:&[u8]|->CallResult{
                
                Ok(msg.to_vec())
            });
            *AC+=1;
        }
     
    };
    ( $(#[$attr:meta])* http_put $name:literal | ($headers:expr) | ($payload:expr)  | $param:tt | $body:block ) => {
        if let Ok(mut AC) = AT_COUNTER2.lock(){
            let mut host = HOST_MAP.lock().unwrap().clone();
            host.push_str($name);
            COMMAND_MAP.lock().unwrap().insert(AC.clone(),host.clone());
            
            let mut headers = $headers;
            let mut req = httparse::Request::new(&mut headers);
            req.method = Some("PUT");
            let http1x = request_to_http1x(&req);
            let r = HttpRequest{
                Http1x:http1x.clone(),
                HttpBody:$payload,
                ProxyUrl:String::from("")
            };
            REQUEST_MAP.lock().unwrap().insert(AC.clone(),r);
            REQUEST_MAR_MAP.lock().unwrap().insert(AC.clone(),http1x);
            RESPONSE_MAR_MAP.lock().unwrap().insert(AC.clone(),|msg:&[u8]|->CallResult{
                
                Ok(msg.to_vec())
            });
            *AC+=1;
        }
     
    };
}
 #[macro_export(local_inner_macros)]
 macro_rules! mock_suite {
     ( name $name:ident ; $($remainder:tt)* ) => {
        wasm_mock_macro::__mock_suite_int!(@int $($remainder)*);
     };
 
     // anonymous mock suite
     ( $($remainder:tt)* ) => {
        wasm_mock_macro::__mock_suite_int!(@int $($remainder)*);
     };
 }
 #[macro_export(local_inner_macros)]
 macro_rules! test_suite {
     ( name $name:ident ;host $host:literal; $($remainder:tt)* ) => {
        let mut host = String::from($host);
        *HOST_MAP.lock().unwrap() = host;
        wasm_mock_macro::__test_suite_int!( @int $($remainder)*);
     };
 
     // anonymous mock suite
     ( $($remainder:tt)* ) => {
        wasm_mock_macro::__test_suite_int!(@int $($remainder)*);
     };
 }
 #[macro_export(local_inner_macros)]
 macro_rules! __mock_suite_int {
     ( @int $(#[$attr:meta])* modify $t:ident $name:literal 
             ($param:ident)
             $body:block
             $($remainder:tt)*
     ) => {
         modify!( $(#[$attr])* $t $name | $param | $body);
         wasm_mock_macro::__mock_suite_int!(@int $($remainder)*);
     };
     
     ( @int $item:item
             $($remainder:tt)*
     ) => {
         $item
         wasm_mock_macro::__mock_suite_int!(@int $($remainder)*);
     };
 
     // internal: empty mock suite
     ( @int ) => { 
     };
 }
 #[macro_export(local_inner_macros)]
 macro_rules! __test_suite_int {
     ( @int $(#[$attr:meta])* test $t:ident $name:literal
            ($headers:expr)
            ($payload:expr)
            ($res:ident)
            $body2:block
            $($remainder:tt)*
     ) => {
         test!( $(#[$attr])* $t $name | ($headers) | ($payload) | $res |$body2);
         wasm_mock_macro::__test_suite_int!( @int $($remainder)*);
     };  
     ( @int $item:item
             $($remainder:tt)*
     ) => {
         $item
         wasm_mock_macro::__test_suite_int!( @int $($remainder)*);
     };
 
     // internal: empty mock suite
     ( @int ) => { 
     };
 }
 #[doc(hidden)]
 #[macro_export]
 macro_rules! _wasm_mock_macro__panic {
     ($($inner:tt)*) => {
         panic!($($inner)*)
     };
 }
 
 #[doc(hidden)]
 #[macro_export]
 macro_rules! _wasm_mock_macro__format {
     ($($inner:tt)*) => {
         format!($($inner)*)
     };
 }
 
 #[doc(hidden)]
 #[macro_export]
 macro_rules! _wasm_mock_macro__println {
     ($($inner:tt)*) => {
         println!($($inner)*)
     };
 }
 #[doc(hidden)]
 #[macro_export]
 macro_rules! _wasm_mock_macro__stringify {
     ($($inner:tt)*) => {
         stringify!($($inner)*)
     };
 }
 #[doc(hidden)]
 #[macro_export]
 macro_rules! _wasm_mock_macro__concat {
     ($($inner:tt)*) => {
         concat!($($inner)*)
     };
 }
 