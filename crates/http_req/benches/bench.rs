#![feature(test)]
extern crate http_req;
extern crate test;

use http_req::{request::Request, response::Response, uri::Uri};
use std::{convert::TryFrom, fs::File, io::Read, time::Duration};
use test::Bencher;

#[bench]
fn parse_response(b: &mut Bencher) {
    let mut content = Vec::new();
    let mut response = File::open("benches/res.txt").unwrap();
    response.read_to_end(&mut content).unwrap();

    b.iter(|| {
        let mut body = Vec::new();
        Response::try_from(&content, &mut body)
    });
}

const URI: &str = "https://www.rust-lang.org/";

#[bench]
fn request_send(b: &mut Bencher) {
    b.iter(|| {
        let uri = Uri::try_from(URI).unwrap();
        let timeout = Some(Duration::from_secs(6));
        let mut writer = Vec::new();

        let res = Request::new(&uri)
            .timeout(timeout)
            .send(&mut writer)
            .unwrap();

        res
    });
}

#[bench]
fn parse_uri(b: &mut Bencher) {
    b.iter(|| Uri::try_from(URI));
}
