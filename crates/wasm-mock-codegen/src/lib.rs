extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{ Span};
use quote::{quote, ToTokens,format_ident};
use std::collections::HashSet as Set;
use syn::fold::{self,Fold};
use syn::{parse_macro_input, ItemFn,Attribute,parse2};
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{ parse_quote, BinOp, Expr, Ident, Local, Pat, Stmt, Token,Lit};

use lazy_static::lazy_static;
use std::error;
use std::sync::{Arc,Mutex};
use std::collections::HashMap;

// lazy_static!{
//     /// HashMap for storing WAPC HandlerSignatures. These will handler signatures will be registered when the host calls save_uid 
//     //static ref REGISTRY: Arc<Mutex<HashMap<String,fn(&[u8]) -> Result<Vec<u8>,Box<dyn error::Error>>>>> = Arc::new(Mutex::new(HashMap::new()));
//     static ref REGISTRY: Arc<Mutex<HashMap<String,fn(&[u8]) -> Result<Vec<u8>,Box<dyn error::Error>>>>> = Arc::new(Mutex::new(HashMap::new()));
// }
#[derive(Debug)]
struct Args {
    //vars: Set<Ident>,
    expr: Expr
}
impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        //println!("input {:?}",input);
        // let has_key = input.peek2(Token![=]);
        // if has_key {
        //     let ident = input.parse::<Ident>()?;
        //     let eq_token = input.parse::<Token![=]>()?;
        //     let expr = input.parse::<Expr>()?;
        //     println!("ident {:?} expr {:?}",ident,expr);
        // } else {
        //     let expr = input.parse::<Expr>()?;
        // }
        let expr = input.parse::<Expr>()?;
        //println!("expr {:?}",expr);
        //let vars = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(Args {
            //vars: vars.into_iter().collect(),
            expr
        })
    }
}

#[proc_macro_attribute]
pub fn http_modify_req(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    // let parsed = parse2::<syn::File>(attr).unwrap();

    // // Extract the attributes from the syntax tree
    // let attributes: Vec<Attribute> = parsed.attrs;
    let mut function = parse_macro_input!(input as ItemFn);
    let mut args = parse_macro_input!(args as Args);
    println!("args {:?}",args);
    // let mut function_name = function.sig.ident.to_string();
    // function_name.push_str("_");
    
    let mut function_name:String = if let Expr::Lit(expr_lit) =  args.expr {
        let v:String =  if let Lit::Str(lit_str) = &expr_lit.lit {
            lit_str.value()
        }else{
            String::from("zz")
        };
        v
    }else{
        String::from("zz")
    };
    function.sig.ident = Ident::new(&function_name, function.sig.ident.span());
    let old_ident = function.sig.ident.clone();
    function_name.push_str("_");
   
    function.sig.ident = Ident::new(&function_name, function.sig.ident.span());
    let new_ident= function.sig.ident.clone();

    let expanded = quote! {
        #function
        //fn #fn_ident() -> &'static str {
        pub fn #old_ident() -> i32 {
            let mut x = 1;
            #new_ident(&mut x);
            x
        }
        //#insert_statements
    };

    // Return the generated code as a TokenStream
    TokenStream::from(expanded)
}
