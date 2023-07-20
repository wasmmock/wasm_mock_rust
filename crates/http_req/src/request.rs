//! creating and sending HTTP requests
#[cfg(feature = "wasmedge_rustls")]
use crate::tls;
use crate::{
    error,
    response::{find_slice, Headers, Response, CR_LF_2,Status,StatusCode},
    uri::Uri,
};
use std::{
    convert::TryFrom,
    fmt,
    io::{self, ErrorKind, Read, Write},
    path::Path,
    time::{Duration, Instant},
};

use wasm_mock_util::*;
use base64::{Engine as _, engine::general_purpose};
use unicase::Ascii;
const CR_LF: &str = "\r\n";
const BUF_SIZE: usize = 8 * 1024;
const SMALL_BUF_SIZE: usize = 8 * 10;
const TEST_FREQ: usize = 100;

///Every iteration increases `count` by one. When `count` is equal to `stop`, `next()`
///returns `Some(true)` (and sets `count` to 0), otherwise returns `Some(false)`.
///Iterator never returns `None`.
pub struct Counter {
    count: usize,
    stop: usize,
}

impl Counter {
    pub const fn new(stop: usize) -> Counter {
        Counter { count: 0, stop }
    }
}

impl Iterator for Counter {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.count += 1;
        let breakpoint = self.count == self.stop;

        if breakpoint {
            self.count = 0;
        }

        Some(breakpoint)
    }
}

///Copies data from `reader` to `writer` until the `deadline` is reached.
///Limitations of current implementation may cause exceeding the deadline.
///Returns how many bytes has been read.
pub fn copy_with_timeout<R, W>(reader: &mut R, writer: &mut W, deadline: Instant) -> io::Result<u64>
where
    R: Read + ?Sized,
    W: Write + ?Sized,
{
    let mut buf = [0; BUF_SIZE];
    let mut copied = 0;
    let mut counter = Counter::new(TEST_FREQ);

    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => return Ok(copied),
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..len])?;
        copied += len as u64;

        if counter.next().unwrap() && Instant::now() >= deadline {
            return Ok(copied);
        }
    }
}

///Copies a given amount of bytes from `reader` to `writer`.
pub fn copy_exact<R, W>(reader: &mut R, writer: &mut W, num_bytes: usize) -> io::Result<()>
where
    R: Read + ?Sized,
    W: Write + ?Sized,
{
    let mut buf = vec![0u8; num_bytes];

    reader.read_exact(&mut buf)?;
    writer.write_all(&mut buf)
}

///Reads data from `reader` and checks for specified `val`ue. When data contains specified value
///or `deadline` is reached, stops reading. Returns read data as array of two vectors: elements
///before and after the `val`.
pub fn copy_until<R>(
    reader: &mut R,
    val: &[u8],
    deadline: Instant,
) -> Result<[Vec<u8>; 2], io::Error>
where
    R: Read + ?Sized,
{
    let mut buf = [0; SMALL_BUF_SIZE];
    let mut writer = Vec::with_capacity(SMALL_BUF_SIZE);
    let mut counter = Counter::new(TEST_FREQ);
    let mut split_idx = 0;

    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        writer.write_all(&buf[..len])?;

        if let Some(i) = find_slice(&writer, val) {
            split_idx = i;
            break;
        }

        if counter.next().unwrap() && Instant::now() >= deadline {
            split_idx = writer.len();
            break;
        }
    }

    Ok([writer[..split_idx].to_vec(), writer[split_idx..].to_vec()])
}

///HTTP request methods
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    OPTIONS,
    PATCH,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Method::*;

        let method = match self {
            GET => "GET",
            HEAD => "HEAD",
            POST => "POST",
            PUT => "PUT",
            DELETE => "DELETE",
            OPTIONS => "OPTIONS",
            PATCH => "PATCH",
        };

        write!(f, "{}", method)
    }
}

///HTTP versions
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum HttpVersion {
    Http10,
    Http11,
    Http20,
}

impl HttpVersion {
    pub const fn as_str(self) -> &'static str {
        use self::HttpVersion::*;

        match self {
            Http10 => "HTTP/1.0",
            Http11 => "HTTP/1.1",
            Http20 => "HTTP/2.0",
        }
    }
}

impl fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

///Relatively low-level struct for making HTTP requests.
///
///It can work with any stream that implements `Read` and `Write`.
///By default it does not close the connection after completion of the response.
///
///# Examples
///```
///use std::{net::TcpStream, convert::TryFrom};
///use http_req::{request::RequestBuilder, tls, uri::Uri, response::StatusCode};
///
///let addr: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
///let mut writer = Vec::new();
///
///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
///let mut stream = tls::Config::default()
///    .connect(addr.host().unwrap_or(""), stream)
///    .unwrap();
///
///let response = RequestBuilder::new(&addr)
///    .header("Connection", "Close")
///    .send(&mut stream, &mut writer)
///    .unwrap();
///
///assert_eq!(response.status_code(), StatusCode::new(200));
///```
#[derive(Clone, Debug, PartialEq)]
pub struct RequestBuilder<'a> {
    uri: &'a Uri<'a>,
    method: Method,
    version: HttpVersion,
    headers: Headers,
    body: Option<&'a [u8]>,
    timeout: Option<Duration>,
}

impl<'a> RequestBuilder<'a> {
    ///Creates new `RequestBuilder` with default parameters
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::RequestBuilder, tls, uri::Uri};
    ///
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn new(uri: &'a Uri<'a>) -> RequestBuilder<'a> {
        RequestBuilder {
            headers: Headers::default_http(uri),
            uri,
            method: Method::GET,
            version: HttpVersion::Http11,
            body: None,
            timeout: None,
        }
    }

    ///Sets request method
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::{RequestBuilder, Method}, tls, uri::Uri};
    ///
    ///let addr= Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .method(Method::HEAD)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn method<T>(&mut self, method: T) -> &mut Self
    where
        Method: From<T>,
    {
        self.method = Method::from(method);
        self
    }

    ///Sets HTTP version
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::{RequestBuilder, HttpVersion}, tls, uri::Uri};
    ///
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .version(HttpVersion::Http10)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```

    pub fn version<T>(&mut self, version: T) -> &mut Self
    where
        HttpVersion: From<T>,
    {
        self.version = HttpVersion::from(version);
        self
    }

    ///Replaces all it's headers with headers passed to the function
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::{RequestBuilder, Method}, response::Headers, tls, uri::Uri};
    ///
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///let mut headers = Headers::new();
    ///headers.insert("Accept-Charset", "utf-8");
    ///headers.insert("Accept-Language", "en-US");
    ///headers.insert("Host", "rust-lang.org");
    ///headers.insert("Connection", "Close");
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .headers(headers)
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn headers<T>(&mut self, headers: T) -> &mut Self
    where
        Headers: From<T>,
    {
        self.headers = Headers::from(headers);
        self
    }

    ///Adds new header to existing/default headers
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::{RequestBuilder, Method}, tls, uri::Uri};
    ///
    ///let addr: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn header<T, U>(&mut self, key: &T, val: &U) -> &mut Self
    where
        T: ToString + ?Sized,
        U: ToString + ?Sized,
    {
        self.headers.insert(key, val);
        self
    }

    ///Sets body for request
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::{RequestBuilder, Method}, tls, uri::Uri};
    ///
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///const body: &[u8; 27] = b"field1=value1&field2=value2";
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .method(Method::POST)
    ///    .body(body)
    ///    .header("Content-Length", &body.len())
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn body(&mut self, body: &'a [u8]) -> &mut Self {
        self.body = Some(body);
        self
    }

    ///Sets timeout for entire connection.
    ///
    ///# Examples
    ///```
    ///use std::{net::TcpStream, time::{Duration, Instant}, convert::TryFrom};
    ///use http_req::{request::RequestBuilder, tls, uri::Uri};
    ///
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///let timeout = Some(Duration::from_secs(3600));
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .timeout(timeout)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn timeout<T>(&mut self, timeout: Option<T>) -> &mut Self
    where
        Duration: From<T>,
    {
        self.timeout = timeout.map(Duration::from);
        self
    }

    ///Sends HTTP request in these steps:
    ///
    ///- Writes request message to `stream`.
    ///- Writes response's body to `writer`.
    ///- Returns response for this request.
    ///
    ///# Examples
    ///
    ///HTTP
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::RequestBuilder, uri::Uri};
    ///
    /// //This address is automatically redirected to HTTPS, so response code will not ever be 200
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///let mut stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    ///
    ///HTTPS
    ///```
    ///use std::{net::TcpStream, convert::TryFrom};
    ///use http_req::{request::RequestBuilder, tls, uri::Uri};
    ///
    ///let addr: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let stream = TcpStream::connect((addr.host().unwrap(), addr.corr_port())).unwrap();
    ///let mut stream = tls::Config::default()
    ///    .connect(addr.host().unwrap_or(""), stream)
    ///    .unwrap();
    ///
    ///let response = RequestBuilder::new(&addr)
    ///    .header("Connection", "Close")
    ///    .send(&mut stream, &mut writer)
    ///    .unwrap();
    ///```
    pub fn send<T, U>(&self, stream: &mut T, writer: &mut U) -> Result<Response, error::Error>
    where
        T: Write + Read,
        U: Write,
    {
        self.write_msg(stream, &self.parse_msg())?;

        let head_deadline = match self.timeout {
            Some(t) => Instant::now() + t,
            None => Instant::now() + Duration::from_secs(360),
        };
        let (res, body_part) = self.read_head(stream, head_deadline)?;

        if self.method == Method::HEAD {
            return Ok(res);
        }

        if let Some(v) = res.headers().get("Transfer-Encoding") {
            if *v == "chunked" {
                let mut dechunked = crate::chunked::Reader::new(body_part.as_slice().chain(stream));

                if let Some(timeout) = self.timeout {
                    let deadline = Instant::now() + timeout;
                    copy_with_timeout(&mut dechunked, writer, deadline)?;
                } else {
                    io::copy(&mut dechunked, writer)?;
                }

                return Ok(res);
            }
        }

        writer.write_all(&body_part)?;

        if let Some(timeout) = self.timeout {
            let deadline = Instant::now() + timeout;
            copy_with_timeout(stream, writer, deadline)?;
        } else {
            let num_bytes = res.content_len();

            match num_bytes {
                Some(0) => {}
                Some(num_bytes) => {
                    copy_exact(stream, writer, num_bytes - body_part.len())?;
                }
                None => {
                    io::copy(stream, writer)?;
                }
            }
        }

        Ok(res)
    }

    ///Writes message to `stream` and flushes it
    pub fn write_msg<T, U>(&self, stream: &mut T, msg: &U) -> Result<(), io::Error>
    where
        T: Write,
        U: AsRef<[u8]>,
    {
        stream.write_all(msg.as_ref())?;
        stream.flush()?;

        Ok(())
    }

    ///Reads head of server's response
    pub fn read_head<T: Read>(
        &self,
        stream: &mut T,
        deadline: Instant,
    ) -> Result<(Response, Vec<u8>), error::Error> {
        let [head, body_part] = copy_until(stream, &CR_LF_2, deadline)?;

        Ok((Response::from_head(&head)?, body_part))
    }

    ///Parses request message for this `RequestBuilder`
    pub fn parse_msg(&self) -> Vec<u8> {
        let request_line = format!(
            "{} {} {}{}",
            self.method,
            self.uri.resource(),
            self.version,
            CR_LF
        );

        let headers: String = self
            .headers
            .iter()
            .map(|(k, v)| format!("{}: {}{}", k.as_ref(), v, CR_LF))
            .collect();

        let mut request_msg = (request_line + &headers + CR_LF).as_bytes().to_vec();

        if let Some(b) = &self.body {
            request_msg.extend(*b);
        }

        request_msg
    }

    ///Consume self to build a `Request` instance.
    ///
    ///# Examples
    ///```
    ///use http_req::{request::RequestBuilder, uri::Uri};
    ///
    ///let addr = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let mut writer = Vec::new();
    ///
    ///let request = RequestBuilder::new(&addr)
    ///    .header("Connection", "Close")
    ///    .build();
    ///
    ///let response = request.send(&mut writer);
    ///```
    ///
    pub fn build(self) -> Request<'a> {
        Request {
            inner: self,
            connect_timeout: Some(Duration::from_secs(60)),
            read_timeout: Some(Duration::from_secs(60)),
            write_timeout: Some(Duration::from_secs(60)),
            root_cert_file_pem: None,
        }
    }
}

///Relatively higher-level struct for making HTTP requests.
///
///It creates stream (`TcpStream` or `TlsStream`) appropriate for the type of uri (`http`/`https`)
///By default it closes connection after completion of the response.
///
///# Examples
///```
///use http_req::{request::Request, uri::Uri, response::StatusCode};
///use std::convert::TryFrom;
///
///let mut writer = Vec::new();
///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
///
///let response = Request::new(&uri).send(&mut writer).unwrap();;
///assert_eq!(response.status_code(), StatusCode::new(200));
///```
///
#[derive(Clone, Debug, PartialEq)]
pub struct Request<'a> {
    inner: RequestBuilder<'a>,
    connect_timeout: Option<Duration>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
    root_cert_file_pem: Option<&'a Path>,
}

impl<'a> Request<'a> {
    ///Creates new `Request` with default parameters
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///
    ///let response = Request::new(&uri).send(&mut writer).unwrap();;
    ///```
    pub fn new(uri: &'a Uri) -> Request<'a> {
        let mut builder = RequestBuilder::new(&uri);
        builder.header("Connection", "Close");

        Request {
            inner: builder,
            connect_timeout: Some(Duration::from_secs(60)),
            read_timeout: Some(Duration::from_secs(60)),
            write_timeout: Some(Duration::from_secs(60)),
            root_cert_file_pem: None,
        }
    }

    ///Sets request method
    ///
    ///# Examples
    ///```
    ///use http_req::{request::{Request, Method}, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///
    ///let response = Request::new(&uri)
    ///    .method(Method::HEAD)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn method<T>(&mut self, method: T) -> &mut Self
    where
        Method: From<T>,
    {
        self.inner.method(method);
        self
    }

    ///Sets HTTP version
    ///
    ///# Examples
    ///```
    ///use http_req::{request::{Request, HttpVersion}, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///
    ///let response = Request::new(&uri)
    ///    .version(HttpVersion::Http10)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```

    pub fn version<T>(&mut self, version: T) -> &mut Self
    where
        HttpVersion: From<T>,
    {
        self.inner.version(version);
        self
    }

    ///Replaces all it's headers with headers passed to the function
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri, response::Headers};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///
    ///let mut headers = Headers::new();
    ///headers.insert("Accept-Charset", "utf-8");
    ///headers.insert("Accept-Language", "en-US");
    ///headers.insert("Host", "rust-lang.org");
    ///headers.insert("Connection", "Close");
    ///
    ///let response = Request::new(&uri)
    ///    .headers(headers)
    ///    .send(&mut writer)
    ///    .unwrap();;
    ///```
    pub fn headers<T>(&mut self, headers: T) -> &mut Self
    where
        Headers: From<T>,
    {
        self.inner.headers(headers);
        self
    }

    ///Adds header to existing/default headers
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///
    ///let response = Request::new(&uri)
    ///    .header("Accept-Language", "en-US")
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn header<T, U>(&mut self, key: &T, val: &U) -> &mut Self
    where
        T: ToString + ?Sized,
        U: ToString + ?Sized,
    {
        self.inner.header(key, val);
        self
    }

    ///Sets body for request
    ///
    ///# Examples
    ///```
    ///use http_req::{request::{Request, Method}, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///const body: &[u8; 27] = b"field1=value1&field2=value2";
    ///
    ///let response = Request::new(&uri)
    ///    .method(Method::POST)
    ///    .header("Content-Length", &body.len())
    ///    .body(body)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn body(&mut self, body: &'a [u8]) -> &mut Self {
        self.inner.body(body);
        self
    }

    ///Sets connection timeout of request.
    ///
    ///# Examples
    ///```
    ///use std::{time::{Duration, Instant}, convert::TryFrom};
    ///use http_req::{request::Request, uri::Uri};
    ///
    ///let mut writer = Vec::new();
    ///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///const body: &[u8; 27] = b"field1=value1&field2=value2";
    ///let timeout = Some(Duration::from_secs(3600));
    ///
    ///let response = Request::new(&uri)
    ///    .timeout(timeout)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn timeout<T>(&mut self, timeout: Option<T>) -> &mut Self
    where
        Duration: From<T>,
    {
        self.inner.timeout = timeout.map(Duration::from);
        self
    }

    ///Sets connect timeout while using internal `TcpStream` instance
    ///
    ///- If there is a timeout, it will be passed to
    ///  [`TcpStream::connect_timeout`][TcpStream::connect_timeout].
    ///- If `None` is provided, [`TcpStream::connect`][TcpStream::connect] will
    ///  be used. A timeout will still be enforced by the operating system, but
    ///  the exact value depends on the platform.
    ///
    ///[TcpStream::connect]: https://doc.rust-lang.org/std/net/struct.TcpStream.html#method.connect
    ///[TcpStream::connect_timeout]: https://doc.rust-lang.org/std/net/struct.TcpStream.html#method.connect_timeout
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri};
    ///use std::{time::Duration, convert::TryFrom};
    ///
    ///let mut writer = Vec::new();
    ///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///const time: Option<Duration> = Some(Duration::from_secs(10));
    ///
    ///let response = Request::new(&uri)
    ///    .connect_timeout(time)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn connect_timeout<T>(&mut self, timeout: Option<T>) -> &mut Self
    where
        Duration: From<T>,
    {
        self.connect_timeout = timeout.map(Duration::from);
        self
    }

    ///Sets read timeout on internal `TcpStream` instance
    ///
    ///`timeout` will be passed to
    ///[`TcpStream::set_read_timeout`][TcpStream::set_read_timeout].
    ///
    ///[TcpStream::set_read_timeout]: https://doc.rust-lang.org/std/net/struct.TcpStream.html#method.set_read_timeout
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri};
    ///use std::{time::Duration, convert::TryFrom};
    ///
    ///let mut writer = Vec::new();
    ///let uri: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///const time: Option<Duration> = Some(Duration::from_secs(15));
    ///
    ///let response = Request::new(&uri)
    ///    .read_timeout(time)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn read_timeout<T>(&mut self, timeout: Option<T>) -> &mut Self
    where
        Duration: From<T>,
    {
        self.read_timeout = timeout.map(Duration::from);
        self
    }

    ///Sets write timeout on internal `TcpStream` instance
    ///
    ///`timeout` will be passed to
    ///[`TcpStream::set_write_timeout`][TcpStream::set_write_timeout].
    ///
    ///[TcpStream::set_write_timeout]: https://doc.rust-lang.org/std/net/struct.TcpStream.html#method.set_write_timeout
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri};
    ///use std::{time::Duration, convert::TryFrom};
    ///
    ///let mut writer = Vec::new();
    ///let uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///const time: Option<Duration> = Some(Duration::from_secs(5));
    ///
    ///let response = Request::new(&uri)
    ///    .write_timeout(time)
    ///    .send(&mut writer)
    ///    .unwrap();
    ///```
    pub fn write_timeout<T>(&mut self, timeout: Option<T>) -> &mut Self
    where
        Duration: From<T>,
    {
        self.write_timeout = timeout.map(Duration::from);
        self
    }

    ///Add a file containing the PEM-encoded certificates that should be added in the trusted root store.
    pub fn root_cert_file_pem(&mut self, file_path: &'a Path) -> &mut Self {
        self.root_cert_file_pem = Some(file_path);
        self
    }

    ///Sends HTTP request.
    ///
    ///Creates `TcpStream` (and wraps it with `TlsStream` if needed). Writes request message
    ///to created stream. Returns response for this request. Writes response's body to `writer`.
    ///
    ///# Examples
    ///```
    ///use http_req::{request::Request, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let mut writer = Vec::new();
    ///let uri: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///
    ///let response = Request::new(&uri).send(&mut writer).unwrap();
    ///```
    pub fn send<T: Write>(&self, writer: &mut T) -> Result<Response, error::Error> {
        let host = self
            .inner
            .uri
            .host()
            .ok_or(error::Error::Parse(error::ParseErr::UriErr))?;
        let port = self.inner.uri.corr_port();

        //#[cfg(target_arch = "wasm32")]
        //foo_http_request($addr:expr,$request:expr,$body:expr,$proxy_url:expr)
        let http1x = format!("{:?} / {:?}",self.inner.method,self.inner.headers);
        let res:HttpResponse = foo_http_request!(self.inner.uri.into(),http1x,String::from(""),String::from(""))?;
        if res.HttpBodyRaw!=String::from(""){
            let buf = general_purpose::STANDARD_NO_PAD.decode(res.HttpBodyRaw)?;
            writer.write(&buf);
            
        }else{
            let buf = res.HttpBody.to_string().as_bytes();
            writer.write(&buf);
          
        }
        let status_code = res.StatusCode.parse::<u16>().unwrap();
        let mut h = Headers::new();
        for (k,v) in res.HttpHeader{
            h.insert(&Ascii::new(k), &v);
        }

        Ok(Response{
            status:Status::new("",StatusCode::new(status_code),""),
            headers:h,
        })
    }
    // fn request_to_http1x(r: &httparse::Request)->String{
    //     let mut owned_string: String = "".to_owned();
    //     if let Some(m) = r.method{
    //       owned_string.push_str(m);
    //       owned_string.push_str(" / HTTP/1.1\r\n");
    //     }
    //     for x in 0..r.headers.len(){
    //       let httparse::Header{name,value} = r.headers[x];
    //       if name!=""{
    //         owned_string.push_str(name);
    //         owned_string.push_str(": ");
    //         owned_string.push_str(std::str::from_utf8(value).unwrap());
    //         owned_string.push_str("\r\n");
    //       }
    //     }
    //     owned_string.push_str("\r\n");
    //     owned_string
    //   }
}

///Creates and sends GET request. Returns response for this request.
///
///# Examples
///```
///use http_req::request;
///
///let mut writer = Vec::new();
///const uri: &str = "https://www.rust-lang.org/learn";
///
///let response = request::get(uri, &mut writer).unwrap();
///```
pub fn get<T: AsRef<str>, U: Write>(uri: T, writer: &mut U) -> Result<Response, error::Error> {
    let uri = Uri::try_from(uri.as_ref())?;

    Request::new(&uri).send(writer)
}

///Creates and sends HEAD request. Returns response for this request.
///
///# Examples
///```
///use http_req::request;
///
///const uri: &str = "https://www.rust-lang.org/learn";
///let response = request::head(uri).unwrap();
///```
pub fn head<T: AsRef<str>>(uri: T) -> Result<Response, error::Error> {
    let mut writer = Vec::new();
    let uri = Uri::try_from(uri.as_ref())?;

    Request::new(&uri).method(Method::HEAD).send(&mut writer)
}

///Creates and sends POST request. Returns response for this request.
///
///# Examples
///```
///use http_req::request;
///
///let mut writer = Vec::new();
///const uri: &str = "https://www.rust-lang.org/learn";
///const body: &[u8; 27] = b"field1=value1&field2=value2";
///
///let response = request::post(uri, body, &mut writer).unwrap();
///```
pub fn post<T: AsRef<str>, U: Write>(
    uri: T,
    body: &[u8],
    writer: &mut U,
) -> Result<Response, error::Error> {
    let uri = Uri::try_from(uri.as_ref())?;

    Request::new(&uri)
        .method(Method::POST)
        .header("Content-Length", &body.len())
        .body(body)
        .send(writer)
}