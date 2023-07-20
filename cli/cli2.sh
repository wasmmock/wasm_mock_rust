#!/bin/bash
Operation="$1"
echo "$Operation"
Port="$4"
if [ "$Port" = '' ]
then
  Port="20825"
fi
MockServer=$(xml sel -t -v "config/$2/mock-server" cli/test_suite.xml)
Mock=$(xml sel -t -v "config/$2/mock" cli/test_suite.xml)
Loop=$(xml sel -t -v "config/$2/loop" cli/test_suite.xml)
if [ "$Operation" = 'v2_mock' ]
then
  cargo build --target wasm32-unknown-unknown --release --example $3
  #wasm-gc target/wasm32-unknown-unknown/release/examples/$3.wasm
  echo "http://$MockServer:$Port/call/v2/mock"
  curl -X POST "http://$MockServer:$Port/call/v2/mock" \
	--header "Content-Type:application/octet-stream" \
	--data-binary "@target/wasm32-unknown-unknown/release/examples/$3.wasm"
else
  cargo build --target wasm32-unknown-unknown --release --example $3
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
