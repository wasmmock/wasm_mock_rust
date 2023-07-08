#[macro_export(local_inner_macros)]
macro_rules! modify {
    ( @parameters | $body:block $test_case_failed:ident ) => { 
        $body 
    };

    ( @parameters , $($remainder:tt)+ ) => {
        modify!(@parameters $($remainder)*);
    };

    ( $(#[$attr:meta])* http_req $name:literal | $param:tt $param_ty:tt $ret_ty:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_http_modify_req",$name),|msg:&[u8]|->CallResult{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = foo_unmarshall::<RequestReceivedInMock>(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            let request = serde_json::to_string(&$param)?;
            Ok(request.as_bytes().to_vec())
        });
    };
    ( $(#[$attr:meta])* http_res $name:literal | $param:tt $param_ty:tt $ret_ty:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_modify_res",$name),|msg:&[u8]|->CallResult{
            let test_case_failed = ::std::cell::Cell::new(false);
            let mut $param = foo_unmarshall::<HttpResponse>(msg)?;
            modify!(@parameters | $($args_and_body)* test_case_failed);
            let request = serde_json::to_string(&$param)?;
            Ok(request.as_bytes().to_vec())
        });
    };
    ( $(#[$attr:meta])* tcp_req $name:literal | $param:tt $param_ty:tt $ret_ty:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_modify_req",$name),|$param:$param_ty|->$ret_ty{
            let test_case_failed = ::std::cell::Cell::new(false);
            return modify!(@parameters | $($args_and_body)* test_case_failed);
        });
    };
    ( $(#[$attr:meta])* tcp_res $name:literal | $param:tt $param_ty:tt $ret_ty:tt | $($args_and_body:tt)* ) => {
        REGISTRY.lock().unwrap().insert(_wasm_mock_macro__format!("{}_tcp_modify_res",$name),|$param:$param_ty|->$ret_ty{
            let test_case_failed = ::std::cell::Cell::new(false);
            return modify!(@parameters | $($args_and_body)* test_case_failed);
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
             ($param:ident : $param_ty:ty) -> $ret_ty:ty
             $body:block
             $($remainder:tt)*
     ) => {
         modify!( $(#[$attr])* $t $name | $param $param_ty $ret_ty | $body);
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
 