//! parsing server response
use crate::{
    error::{Error, ParseErr},
    uri::Uri,
};
use std::{
    collections::{hash_map, HashMap},
    fmt,
    io::Write,
    str,
};
use unicase::Ascii;

pub(crate) const CR_LF_2: [u8; 4] = [13, 10, 13, 10];

///Represents an HTTP response.
///
///It contains `Headers` and `Status` parsed from response.
#[derive(Debug, PartialEq, Clone)]
pub struct Response {
    status: Status,
    headers: Headers,
}

impl Response {
    ///Creates new `Response` with head - status and headers - parsed from a slice of bytes
    ///
    ///# Examples
    ///```
    ///use http_req::response::Response;
    ///
    ///const HEAD: &[u8; 102] = b"HTTP/1.1 200 OK\r\n\
    ///                         Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                         Content-Type: text/html\r\n\
    ///                         Content-Length: 100\r\n\r\n";
    ///
    ///let response = Response::from_head(HEAD).unwrap();
    ///```
    pub fn from_head(head: &[u8]) -> Result<Response, Error> {
        let mut head = str::from_utf8(head)?.splitn(2, '\n');

        let status = head.next().ok_or(ParseErr::StatusErr)?.parse()?;
        let headers = head.next().ok_or(ParseErr::HeadersErr)?.parse()?;

        Ok(Response { status, headers })
    }

    ///Parses `Response` from slice of bytes. Writes it's body to `writer`.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Response;
    ///
    ///const RESPONSE: &[u8; 129] = b"HTTP/1.1 200 OK\r\n\
    ///                             Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                             Content-Type: text/html\r\n\
    ///                             Content-Length: 100\r\n\r\n\
    ///                             <html>hello\r\n\r\nhello</html>";
    ///let mut body = Vec::new();
    ///
    ///let response = Response::try_from(RESPONSE, &mut body).unwrap();
    ///```
    pub fn try_from<T: Write>(res: &[u8], writer: &mut T) -> Result<Response, Error> {
        if res.is_empty() {
            Err(Error::Parse(ParseErr::Empty))
        } else {
            let pos = match find_slice(res, &CR_LF_2) {
                Some(v) => v,
                None => res.len(),
            };

            let response = Self::from_head(&res[..pos])?;
            writer.write_all(&res[pos..])?;

            Ok(response)
        }
    }

    ///Returns status code of this `Response`.
    ///
    ///# Examples
    ///```
    ///use http_req::response::{Response, StatusCode};
    ///
    ///const RESPONSE: &[u8; 129] = b"HTTP/1.1 200 OK\r\n\
    ///                             Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                             Content-Type: text/html\r\n\
    ///                             Content-Length: 100\r\n\r\n\
    ///                             <html>hello\r\n\r\nhello</html>";
    ///let mut body = Vec::new();
    ///
    ///let response = Response::try_from(RESPONSE, &mut body).unwrap();
    ///assert_eq!(response.status_code(), StatusCode::new(200));
    ///```
    pub const fn status_code(&self) -> StatusCode {
        self.status.code
    }

    ///Returns HTTP version of this `Response`.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Response;
    ///
    ///const RESPONSE: &[u8; 129] = b"HTTP/1.1 200 OK\r\n\
    ///                             Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                             Content-Type: text/html\r\n\
    ///                             Content-Length: 100\r\n\r\n\
    ///                             <html>hello\r\n\r\nhello</html>";
    ///let mut body = Vec::new();
    ///
    ///let response = Response::try_from(RESPONSE, &mut body).unwrap();
    ///assert_eq!(response.version(), "HTTP/1.1");
    ///```
    pub fn version(&self) -> &str {
        &self.status.version
    }

    ///Returns reason of this `Response`.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Response;
    ///
    ///const RESPONSE: &[u8; 129] = b"HTTP/1.1 200 OK\r\n\
    ///                             Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                             Content-Type: text/html\r\n\
    ///                             Content-Length: 100\r\n\r\n\
    ///                             <html>hello\r\n\r\nhello</html>";
    ///let mut body = Vec::new();
    ///
    ///let response = Response::try_from(RESPONSE, &mut body).unwrap();
    ///assert_eq!(response.reason(), "OK");
    ///```
    pub fn reason(&self) -> &str {
        &self.status.reason
    }

    ///Returns headers of this `Response`.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Response;
    ///
    ///const RESPONSE: &[u8; 129] = b"HTTP/1.1 200 OK\r\n\
    ///                             Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                             Content-Type: text/html\r\n\
    ///                             Content-Length: 100\r\n\r\n\
    ///                             <html>hello\r\n\r\nhello</html>";
    ///let mut body = Vec::new();
    ///
    ///let response = Response::try_from(RESPONSE, &mut body).unwrap();
    ///let headers = response.headers();
    ///```
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    ///Returns length of the content of this `Response` as a `Option`, according to information
    ///included in headers. If there is no such an information, returns `None`.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Response;
    ///
    ///const RESPONSE: &[u8; 129] = b"HTTP/1.1 200 OK\r\n\
    ///                             Date: Sat, 11 Jan 2003 02:44:04 GMT\r\n\
    ///                             Content-Type: text/html\r\n\
    ///                             Content-Length: 100\r\n\r\n\
    ///                             <html>hello\r\n\r\nhello</html>";
    ///let mut body = Vec::new();
    ///
    ///let response = Response::try_from(RESPONSE, &mut body).unwrap();
    ///assert_eq!(response.content_len().unwrap(), 100);
    ///```
    pub fn content_len(&self) -> Option<usize> {
        self.headers()
            .get("Content-Length")
            .and_then(|len| len.parse().ok())
            .or_else(|| {
                if self.status.code.0 == 204 {
                    Some(0)
                } else {
                    None
                }
            })
    }
}

///Status of HTTP response
#[derive(PartialEq, Debug, Clone)]
pub struct Status {
    version: String,
    code: StatusCode,
    reason: String,
}

impl Status {
    pub fn new(version: &str, code: StatusCode, reason: &str) -> Status {
        Status::from((version, code, reason))
    }
}

impl<T, U, V> From<(T, U, V)> for Status
where
    T: ToString,
    V: ToString,
    StatusCode: From<U>,
{
    fn from(status: (T, U, V)) -> Status {
        Status {
            version: status.0.to_string(),
            code: StatusCode::from(status.1),
            reason: status.2.to_string(),
        }
    }
}

impl str::FromStr for Status {
    type Err = ParseErr;

    fn from_str(status_line: &str) -> Result<Status, Self::Err> {
        let mut status_line = status_line.trim().splitn(3, ' ');

        let version = status_line.next().ok_or(ParseErr::StatusErr)?;
        let code: StatusCode = status_line.next().ok_or(ParseErr::StatusErr)?.parse()?;
        let reason = match status_line.next() {
            Some(reason) => reason,
            None => code.reason().unwrap_or("Unknown"),
        };

        Ok(Status::from((version, code, reason)))
    }
}

///Wrapper around HashMap<Ascii<String>, String> with additional functionality for parsing HTTP headers
///
///# Example
///```
///use http_req::response::Headers;
///
///let mut headers = Headers::new();
///headers.insert("Connection", "Close");
///
///assert_eq!(headers.get("Connection"), Some(&"Close".to_string()))
///```
#[derive(Debug, PartialEq, Clone, Default)]
pub struct Headers(HashMap<Ascii<String>, String>);

impl Headers {
    ///Creates an empty `Headers`.
    ///
    ///The headers are initially created with a capacity of 0, so they will not allocate until
    ///it is first inserted into.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Headers;
    ///
    ///let mut headers = Headers::new();
    ///```
    pub fn new() -> Headers {
        Headers(HashMap::new())
    }

    ///Creates empty `Headers` with the specified capacity.
    ///
    ///The headers will be able to hold at least capacity elements without reallocating.
    ///If capacity is 0, the headers will not allocate.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Headers;
    ///
    ///let mut headers = Headers::with_capacity(200);
    ///```
    pub fn with_capacity(capacity: usize) -> Headers {
        Headers(HashMap::with_capacity(capacity))
    }

    ///An iterator visiting all key-value pairs in arbitrary order.
    ///The iterator's element type is (&Ascii<String>, &String).
    ///
    ///# Examples
    ///```
    ///use http_req::response::Headers;
    ///
    ///let mut headers = Headers::new();
    ///headers.insert("Accept-Charset", "utf-8");
    ///headers.insert("Accept-Language", "en-US");
    ///headers.insert("Connection", "Close");
    ///
    ///let mut iterator = headers.iter();
    ///```
    pub fn iter(&self) -> hash_map::Iter<Ascii<String>, String> {
        self.0.iter()
    }

    ///Returns a reference to the value corresponding to the key.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Headers;
    ///
    ///let mut headers = Headers::new();
    ///headers.insert("Accept-Charset", "utf-8");
    ///
    ///assert_eq!(headers.get("Accept-Charset"), Some(&"utf-8".to_string()))
    ///```
    pub fn get<T: ToString + ?Sized>(&self, k: &T) -> Option<&std::string::String> {
        self.0.get(&Ascii::new(k.to_string()))
    }

    ///Inserts a key-value pair into the headers.
    ///
    ///If the headers did not have this key present, None is returned.
    ///
    ///If the headers did have this key present, the value is updated, and the old value is returned.
    ///The key is not updated, though; this matters for types that can be == without being identical.
    ///
    ///# Examples
    ///```
    ///use http_req::response::Headers;
    ///
    ///let mut headers = Headers::new();
    ///headers.insert("Accept-Language", "en-US");
    ///```
    pub fn insert<T, U>(&mut self, key: &T, val: &U) -> Option<String>
    where
        T: ToString + ?Sized,
        U: ToString + ?Sized,
    {
        self.0.insert(Ascii::new(key.to_string()), val.to_string())
    }

    ///Creates default headers for a HTTP request
    ///
    ///# Examples
    ///```
    ///use http_req::{response::Headers, uri::Uri};
    ///use std::convert::TryFrom;
    ///
    ///let uri: Uri = Uri::try_from("https://www.rust-lang.org/learn").unwrap();
    ///let headers = Headers::default_http(&uri);
    ///```
    pub fn default_http(uri: &Uri) -> Headers {
        let mut headers = Headers::with_capacity(4);

        headers.insert("Host", &uri.host_header().unwrap_or_default());
        headers.insert("Referer", &uri);

        headers
    }
}

impl str::FromStr for Headers {
    type Err = ParseErr;

    fn from_str(s: &str) -> Result<Headers, ParseErr> {
        let headers = s.trim();

        if headers.lines().all(|e| e.contains(':')) {
            let headers = headers
                .lines()
                .map(|elem| {
                    let idx = elem.find(':').unwrap();
                    let (key, value) = elem.split_at(idx);
                    (Ascii::new(key.to_string()), value[1..].trim().to_string())
                })
                .collect();

            Ok(Headers(headers))
        } else {
            Err(ParseErr::HeadersErr)
        }
    }
}

impl From<HashMap<Ascii<String>, String>> for Headers {
    fn from(map: HashMap<Ascii<String>, String>) -> Headers {
        Headers(map)
    }
}

impl From<Headers> for HashMap<Ascii<String>, String> {
    fn from(map: Headers) -> HashMap<Ascii<String>, String> {
        map.0
    }
}

impl fmt::Display for Headers {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let headers: String = self
            .iter()
            .map(|(key, val)| format!("  {}: {}\r\n", key, val))
            .collect();

        write!(f, "{{\r\n{}}}", headers)
    }
}

///Code sent by a server in response to a client's request.
///
///# Example
///```
///use http_req::response::StatusCode;
///
///const code: StatusCode = StatusCode::new(200);
///assert!(code.is_success())
///```
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct StatusCode(u16);

impl StatusCode {
    ///Creates new StatusCode from `u16` value.
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(200);
    ///```
    pub const fn new(code: u16) -> StatusCode {
        StatusCode(code)
    }

    ///Checks if this `StatusCode` is within 100-199, which indicates that it's Informational.
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(101);
    ///assert!(code.is_info())
    ///```
    pub const fn is_info(self) -> bool {
        self.0 >= 100 && self.0 < 200
    }

    ///Checks if this `StatusCode` is within 200-299, which indicates that it's Successful.
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(204);
    ///assert!(code.is_success())
    ///```
    pub const fn is_success(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    ///Checks if this `StatusCode` is within 300-399, which indicates that it's Redirection.
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(301);
    ///assert!(code.is_redirect())
    ///```
    pub const fn is_redirect(self) -> bool {
        self.0 >= 300 && self.0 < 400
    }

    ///Checks if this `StatusCode` is within 400-499, which indicates that it's Client Error.
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(400);
    ///assert!(code.is_client_err())
    ///```
    pub const fn is_client_err(self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    ///Checks if this `StatusCode` is within 500-599, which indicates that it's Server Error.
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(503);
    ///assert!(code.is_server_err())
    ///```
    pub const fn is_server_err(self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    ///Checks this `StatusCode` using closure `f`
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(203);
    ///assert!(code.is(|i| i > 199 && i < 250))
    ///```
    pub fn is<F: FnOnce(u16) -> bool>(self, f: F) -> bool {
        f(self.0)
    }

    ///Returns `Reason-Phrase` corresponding to this `StatusCode`
    ///
    ///# Examples
    ///```
    ///use http_req::response::StatusCode;
    ///
    ///const code: StatusCode = StatusCode::new(200);
    ///assert_eq!(code.reason(), Some("OK"))
    ///```
    pub const fn reason(self) -> Option<&'static str> {
        let reason = match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            102 => "Processing",
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            203 => "Non Authoritative Information",
            204 => "No Content",
            205 => "Reset Content",
            206 => "Partial Content",
            207 => "Multi-Status",
            208 => "Already Reported",
            226 => "IM Used",
            300 => "Multiple Choices",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            305 => "Use Proxy",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            402 => "Payment Required",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            407 => "Proxy Authentication Required",
            408 => "Request Timeout",
            409 => "Conflict",
            410 => "Gone",
            411 => "Length Required",
            412 => "Precondition Failed",
            413 => "Payload Too Large",
            414 => "URI Too Long",
            415 => "Unsupported Media Type",
            416 => "Range Not Satisfiable",
            417 => "Expectation Failed",
            418 => "I'm a teapot",
            421 => "Misdirected Request",
            422 => "Unprocessable Entity",
            423 => "Locked",
            424 => "Failed Dependency",
            426 => "Upgrade Required",
            428 => "Precondition Required",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            451 => "Unavailable For Legal Reasons",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            505 => "HTTP Version Not Supported",
            506 => "Variant Also Negotiates",
            507 => "Insufficient Storage",
            508 => "Loop Detected",
            510 => "Not Extended",
            511 => "Network Authentication Required",
            _ => "",
        };

        if !reason.is_empty() {
            Some(reason)
        } else {
            None
        }
    }
}

impl From<StatusCode> for u16 {
    fn from(code: StatusCode) -> Self {
        code.0
    }
}

impl From<u16> for StatusCode {
    fn from(code: u16) -> Self {
        StatusCode(code)
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl str::FromStr for StatusCode {
    type Err = ParseErr;

    fn from_str(s: &str) -> Result<StatusCode, ParseErr> {
        Ok(StatusCode::new(s.parse()?))
    }
}

///Finds elements slice `e` inside slice `data`. Returns position of the end of first match.
pub fn find_slice<T>(data: &[T], e: &[T]) -> Option<usize>
where
    [T]: PartialEq,
{
    if data.len() > e.len() {
        for i in 0..=data.len() - e.len() {
            if data[i..(i + e.len())] == *e {
                return Some(i + e.len());
            }
        }
    }

    None
}
