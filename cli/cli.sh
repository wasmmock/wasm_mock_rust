#!/bin/bash
Operation="$1"
Port="$4"
if [ "$Port" = '' ]
then
  Port="20825"
fi
MockServer=$(xml sel -t -v "config/$2/mock-server" cli/test_suite.xml)
Mock=$(xml sel -t -v "config/$2/mock" cli/test_suite.xml)
Loop=$(xml sel -t -v "config/$2/loop" cli/test_suite.xml)

# Build, and STOP if it fails. Every branch below uploads
# target/.../$3.wasm unconditionally, so a failed build used to ship the
# PREVIOUS artifact and the server still answered {"Message":"ok"} — a green
# deploy serving a binary that never contained your change. Not `set -e`: the
# final branch's report wget/jq loop is allowed to fail without killing the run.
build() {
  if ! cargo build --target wasm32-unknown-unknown --release --example "$1"
  then
    echo "cli.sh: build failed for example '$1' — refusing to upload a stale wasm" >&2
    exit 1
  fi
}
if [ "$Operation" = 'set_mock' ] || [ "$Operation" = 'set_mock_http' ] || [ "$Operation" = 'set_base_ws_mock' ] || [ "$Operation" = 'create_ws_mock' ] || [ "$Operation" = 'set_base_ws_call' ] || [ "$Operation" = 'set_mock_fiddler' ] || [ "$Operation" = 'set_mock_tcp_fiddler' ]
then
  build "$3"
  #wasm-gc target/wasm32-unknown-unknown/release/examples/$3.wasm
  curl -X POST "http://$MockServer:$Port/call/$Operation?targets=$Mock" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$3.wasm"
elif [ "$Operation" = 'set_mock_tcp' ]
then
  Ports=$(xml sel -t -v "config/$2/ports" cli/test_suite.xml)
  curl -X POST "http://$MockServer:$Port/call/$Operation?targets=$Mock&ports=$Ports" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$3.wasm"
elif [ "$Operation" = 'rpc_lite' ]
then
  build "$3"
  #wasm-gc target/wasm32-unknown-unknown/release/examples/$3.wasm
  curl -m 15 -X POST "http://$MockServer:$Port/call/rpc?loop=$Loop&targets=$Mock" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$3.wasm"
elif [ "$Operation" = 'stress' ]
then
  build "$3"
  #wasm-gc target/wasm32-unknown-unknown/release/examples/$3.wasm
elif [ "$Operation" = 'fiddler' ] || [ "$Operation" = 'tcp_fiddler' ]
then
  build "$3"
  #wasm-gc target/wasm32-unknown-unknown/release/examples/$3.wasm
  # mock_targets (not targets) is what CallFiddler/CallTcpFiddler read to key the
  # report; without it the report is stored under "" and comes back with no hits
  curl "http://$MockServer:$Port/call/$Operation?targets=$Mock&mock_targets=$Mock&duration=$5" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$3.wasm"
else
  build "$3"
  #wasm-gc target/wasm32-unknown-unknown/release/examples/$3.wasm
  echo "$MockServer"
  echo "$Operation"
  curl -m 100 -X POST "http://$MockServer:$Port/call/$Operation?loop=$Loop&targets=$Mock" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$3.wasm" | jq '.Header.ReportId' > reporttemp.txt
  sed 's/\"//g' reporttemp.txt > reportid.txt
  reportid="reportid.txt"
  while IFS= read -r line
  do
    cat class_id_map.json | jq ". +=[{\"uid\":\"$line\",\"class\":\"$2\"}]" > class_id_map2.json
    cp class_id_map2.json class_id_map.json
    wget -O "report/$line.html" "http://$MockServer:$Port/report/$line"
    wget -O "report_data/$line.json" "http://$MockServer:$Port/report_data/$line"
  done < "$reportid"
fi
