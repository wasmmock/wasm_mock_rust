#!/bin/bash
# Bring the octos mock harness up, and prove it is actually serving.
#
#   bash scripts/harness_up.sh [scenario]
#
# Idempotent: re-running redeploys the wasm and leaves one of everything. With
# no argument the recorded baseline is installed; pass a scenario name (see
# `setup_scenario.py --list`) to deploy that data set instead.
#
# Run this from a normal shell. It leaves three daemons behind, so an agent
# harness that waits on the process group (Claude Code's Bash tool does) will
# sit there until it is killed — and killing it takes the whole harness down
# with it. From such a tool, launch it detached / in the background instead.
#
# What it leaves running:
#   :20825  wasm mock server (Go)      -- hosts the guests, serves reports
#   :3341   mock_octos                 -- answers the UI Protocol itself
#   :3340   fiddler_ws_3340            -- passthrough recorder in front of :3341
#
# Point octoscode at the FIDDLER, never at :3341 directly:
#   octoscode --profile-id alan --session 'alan:local:tui#coding' \
#     --endpoint ws://127.0.0.1:3340/api/ui-protocol/ws --auth-token local-dev-token
set -u

MOCK=/Users/alanpoon/Documents/go/wasm_mock_rust
SERVER=/Users/alanpoon/Documents/go/wasm_mock_server
STATE=${HARNESS_STATE:-/tmp/octos-harness}
# Fixed, not configurable: 3340 and 3341 are baked into the guests' port maps
# (`port_map!()` in ws_3340.rs / mock_octos.rs) and mirrored in
# cli/test_suite.xml. Changing the port here alone would leave the host looking
# for a guest registered under a name nothing exports, and the connection would
# simply hang.
FIDDLER_PORT=3340
MOCK_PORT=3341
SCEN=${1:-}

mkdir -p "$STATE"

fail() { echo "harness_up: $*" >&2; exit 1; }

# ---- prerequisites ------------------------------------------------------
# Checked up front and by name: a missing `xml` surfaces deep inside cli.sh as
# an empty port map and a mock deployed against nothing, which is a much worse
# error message than this one.
for c in go cargo rustup xml python3 tmux curl; do
  command -v "$c" >/dev/null || fail "missing prerequisite: $c (xml is xmlstarlet)"
done
rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown \
  || fail "missing rust target: rustup target add wasm32-unknown-unknown"
[ -d "$SERVER" ] || fail "mock server not found at $SERVER"

# ---- mock server (:20825) ----------------------------------------------
if curl -s -m 3 -o /dev/null "http://localhost:20825/" 2>/dev/null; then
  echo "mock server already up on :20825"
else
  echo "starting mock server..."
  ( cd "$SERVER" && nohup go run runner/main.go > "$STATE/mockserver.log" 2>&1 < /dev/null & )
  for _ in $(seq 1 40); do
    sleep 1
    curl -s -m 2 -o /dev/null "http://localhost:20825/" 2>/dev/null && break
  done
  curl -s -m 3 -o /dev/null "http://localhost:20825/" 2>/dev/null \
    || fail "mock server never came up; see $STATE/mockserver.log"
  echo "mock server up on :20825"
fi

# ---- data set + deploy (:3341) -----------------------------------------
cd "$MOCK" || fail "cannot cd $MOCK"
if [ -n "$SCEN" ]; then
  python3 examples/automation/octos/setup_scenario.py "$SCEN" >/dev/null \
    || fail "no such scenario: $SCEN"
  echo "installed scenario: $SCEN"
else
  python3 examples/automation/octos/setup_scenario.py --restore >/dev/null \
    || fail "could not restore the baseline"
  echo "installed scenario: pristine (baseline)"
fi

echo "building + deploying mock_octos..."
bash cli/mock_octos.sh >"$STATE/deploy.log" 2>&1 \
  || fail "deploy failed; see $STATE/deploy.log"
sleep 2

# ---- fiddler (:3340) ----------------------------------------------------
# AFTER the mock, never before: PoolInit resolves and dials the remote half of
# the port map at install time, and the goroutine that reads the client will not
# start until that dial lands. Install this against a dead :3341 and the guest
# is never invoked at all.
echo "installing fiddler 3340 -> $MOCK_PORT..."
bash cli/mock_ws_3340.sh >"$STATE/fiddler.log" 2>&1 \
  || fail "fiddler install failed; see $STATE/fiddler.log"
sleep 2

# ---- prove it serves ----------------------------------------------------
# A deploy that reports ok and a mock that answers are different claims. Ask
# the guest which data set it thinks it is serving, through the FIDDLER, exactly
# the way the client will — that also proves the chained hop forwards.
python3 - "$FIDDLER_PORT" <<'PY' || fail "harness is up but not answering"
import json, sys, time, os, socket, base64, struct
port = int(sys.argv[1])
s = socket.create_connection(("127.0.0.1", port), 10)
key = base64.b64encode(os.urandom(16)).decode()
s.sendall(f"GET /api/ui-protocol/ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
          "Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n"
          f"Sec-WebSocket-Key: {key}\r\nx-profile-id: alan\r\n"
          "authorization: Bearer local-dev-token\r\n\r\n".encode())
buf = b""
s.settimeout(10)
while b"\r\n\r\n" not in buf:
    buf += s.recv(4096)
if b"101" not in buf.split(b"\r\n")[0]:
    raise SystemExit("handshake refused")
body = json.dumps({"jsonrpc": "2.0", "id": "up", "method": "server/heartbeat",
                   "params": {}}).encode()
mask = os.urandom(4)
hdr = bytearray([0x81, 0x80 | len(body)]) + mask
hdr += bytes(b ^ mask[i % 4] for i, b in enumerate(body))
s.sendall(bytes(hdr))
buf = buf.split(b"\r\n\r\n", 1)[1]
end = time.time() + 10
while time.time() < end:
    if len(buf) >= 2:
        ln = buf[1] & 0x7F
        off = 2
        if ln == 126:
            ln = struct.unpack(">H", buf[2:4])[0]; off = 4
        elif ln == 127:
            ln = struct.unpack(">Q", buf[2:10])[0]; off = 10
        if len(buf) >= off + ln:
            r = json.loads(buf[off:off + ln].decode())["result"]
            print(f"  serving: {r.get('scenario')}  [{r.get('area')}]")
            print(f"  expects: {r.get('expectation','(none)')}")
            raise SystemExit(0)
    buf += s.recv(65536)
raise SystemExit("no heartbeat reply")
PY

cat <<EOF

harness up.
  :20825  mock server     ($STATE/mockserver.log)
  :$MOCK_PORT   mock_octos      ($STATE/deploy.log)
  :$FIDDLER_PORT   fiddler         ($STATE/fiddler.log)

run octoscode against the FIDDLER:
  cd /Users/alanpoon/Documents/rust/robius/octoscode
  ./target/debug/octoscode --profile-id alan --session 'alan:local:tui#coding' \\
    --endpoint ws://127.0.0.1:$FIDDLER_PORT/api/ui-protocol/ws --auth-token local-dev-token

drive a specific build (scripts/drive.sh honours the same variable):
  OCTOSCODE_BIN=\$OCTOSCODE/target/release/octoscode bash scripts/drive.sh pristine yes

tear down with: bash scripts/harness_down.sh
EOF
