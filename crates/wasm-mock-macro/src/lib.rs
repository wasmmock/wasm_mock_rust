#[macro_export(local_inner_macros)]
macro_rules! modify {
    ( @parameters | $body:block $test_case_failed:ident ) => { 
        $body 
    };

    ( @parameters , $($remainder:tt)+ ) => {
        modify!(@parameters $($remainder)*);
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
    ( $(#[$attr:meta])* tcp_req $name:literal | $param:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_modify_req",$name),|$param:$param_ty|{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = rmp_serde::from_read_ref::<TcpPayload>(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            if $param.Tcp_Items.len()==0{
                return Ok(b"/continue".to_vec())
            }
            Ok(rmp_serde::to_vec(&$param))
        });
    };
    ( $(#[$attr:meta])* tcp_res $name:literal | $param:tt| $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_modify_res",$name),|$param:$param_ty|{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = rmp_serde::from_read_ref::<TcpPayload>(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            if $param.Tcp_Items.len()==0{
                return Ok(b"/continue".to_vec())
            }
            Ok(rmp_serde::to_vec(&$param))
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

    ( $(#[$attr:meta])* http_get | $name:literal | ($headers:expr) | $param:tt | $body:block ) => {
        if let Ok(mut AC) = AT_COUNTER2.lock(){
            
            COMMAND_MAP.lock().unwrap().insert(AC.clone(),$name.to_string());
            
            let mut headers = $headers;
            let mut req = httparse::Request::new(&mut headers);
            let http1x = request_to_http1x(&req);
            let r = HttpRequest{
                Http1x:http1x.clone(),
                HttpBody:Vec::new(),
                ProxyUrl:String::from("")
            };
            REQUEST_MAP.lock().unwrap().insert(AC.clone(),r);
            REQUEST_MAR_MAP.lock().unwrap().insert(AC.clone(),http1x);
            RESPONSE_MAR_MAP.lock().unwrap().insert(AC.clone(),|res:HttpResponse|{
                let $param = res;
                $body
            });
            // let mut writer = Vec::new();
            // let req = $body;
            // let z = req.clone().parse_msg();
            // let res = req.clone().build().send(&mut writer).unwrap();
            // let test_case_failed = ::std::cell::Cell::new(false);
            // //$args_and_body
            // test!(@parameters | $($args_and_body)* test_case_failed);
            *AC+=1;
        }
        //COMMAND_MAP.lock().unwrap().insert(1,$name);
        
        // REGISTRY.lock().unwrap().insert("command",|msg:&[u8]|->CallResult{

        // });
        // REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_http_modify_req",$name),|msg:&[u8]|->CallResult{
        //     let test_case_failed = ::std::cell::Cell::new(false);
        //     let mut $param = foo_unmarshall::<RequestReceivedInMock>(msg)?;
        //     //let mut $param = foo_unmarshall::<$param_ty>(msg)?;
        //     test!(@parameters | $($args_and_body)* test_case_failed);
        //     let request = serde_json::to_string(&$param)?;
        //     Ok(request.as_bytes().to_vec())
        // });
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
     ( name $name:ident ; $($remainder:tt)* ) => {
        wasm_mock_macro::__test_suite_int!(@int $($remainder)*);
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
            ($res:ident)
            $body2:block
            $($remainder:tt)*
     ) => {
         test!( $(#[$attr])* $t | $name | ($headers) | $res |$body2);
         wasm_mock_macro::__test_suite_int!(@int $($remainder)*);
     };  
     ( @int $item:item
             $($remainder:tt)*
     ) => {
         $item
         wasm_mock_macro::__test_suite_int!(@int $($remainder)*);
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
 