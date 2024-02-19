#!/bin/bash
Operation="$1"
echo "$Operation"
Port="$3"
if [ "$Port" = '' ]
then
  Port="20825"
fi
cargo build --target wasm32-unknown-unknown --release --example $1
curl -X POST "http://localhost:$Port/call/v2/unified?dst=http://localhost:20825&duration=$2" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@../../target/wasm32-unknown-unknown/release/examples/$1.wasm"