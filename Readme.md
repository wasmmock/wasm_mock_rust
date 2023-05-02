
A collection of crates for Automation and Mocking in Wasm Mock Server
===========================

<!-- ![alt text for screen readers](/crates/wasm-mock-util/hammock-min.png "Text to show on mouseover" | width=100) -->
<!-- <img src="/crates/wasm-mock-util/hammock-min.png" width="100" height="100"> -->
Wasm Mock Util is a collection of macros and functions that are built on top of WAPC protocol used in Wasm Mock Server. Wasm Mock Server accepts user's defined webassembly for mocking and automation via post request.

Cargo.toml
```toml
wasm-mock-util = "0.1.0"
```

```bash
Operation=set_mock_fiddler
Port=20825
Mock=/hello,\/v2\/seasons\/.*\/competitions
curl -X POST "http://$MockServer:$Port/call/$Operation?targets=$Mock" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$wasm_file.wasm"
```
## Mocking
Url Parameter: targets
| Operation  | Explaination | Targets (comma separated) | Targets Example | 
| ------------- | ------------- | ------------- | ------------- |
| set_mock_fiddler  | Mock Http Proxy by path  | paths | /hello,\/v2\/seasons\/.*\/competitions |
| set_mock_tcp_fiddler  | Mock TCP Proxy by local connection and remote connection | {local port}-:{remote port}  | 3335-:3334 |

## Recording fiddler request and response
Url Parameter: targets, duration(in sec)
Returns report id
| Operation  | Explaination | Targets (comma separated) | Targets Example |
| ------------- | ------------- | ------------- | ------------- |
| fiddler  | Record HTTP fiddler in a report for a duration | paths | /hello,\/v2\/seasons\/.*\/competitions |
| tcp_fiddler  | Record Tcp fiddler in a report for a duration | {local port}-:{remote port}  | 3335-:3334 |

## Automation
Url Parameter: targets, loop
Returns report id
| Operation  | Explaination |
| ------------- | ------------- |
| http  | For HTTP Automation | 
| rpc  | For RPC Automation only used with customization of wasm mock server |

## Report
Report id 
html -> http://$MockServer:$Port/report/${report id}
json -> http://$MockServer:$Port/report_data/${report id}.json


```mermaid
graph TD;
    A-->B;
    A-->C;
    B-->D;
    C-->D;
```