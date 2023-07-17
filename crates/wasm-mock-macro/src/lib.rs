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
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_modify_res",$name),|msg:&[u8]|->CallResult{
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
 macro_rules! __test_suite_int {
     ( @int $(#[$attr:meta])* modify $t:ident $name:literal 
             ($param:ident)
             $body:block
             $($remainder:tt)*
     ) => {
         modify!( $(#[$attr])* $t $name | $param | $body);
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
 