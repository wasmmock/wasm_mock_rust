#!/bin/bash
# Stop everything harness_up.sh started, and put the baseline data back.
#
#   bash scripts/harness_down.sh [--keep-data]
#
# The data set is restored by default: a scenario left installed makes
# `cargo test --example mock_octos` fail against fixtures it never agreed to,
# which reads as a broken build rather than a dirty working tree.
set -u

MOCK=/Users/alanpoon/Documents/go/wasm_mock_rust
FIDDLER_PORT=3340
MOCK_PORT=3341

# wstap is no longer part of the harness; kill a stray one anyway so an old
# session cannot keep :3340 and mask the fiddler failing to bind.
pkill -f "wstap.py $FIDDLER_PORT" 2>/dev/null && echo "stopped stray tap on :$FIDDLER_PORT"
pkill -f 'go run runner/main.go' 2>/dev/null && echo "stopped mock server runner"
pkill -f 'go-build.*/main$' 2>/dev/null && echo "stopped mock server"
tmux kill-session -t sc 2>/dev/null && echo "killed tmux session sc"
sleep 2

if [ "${1:-}" != "--keep-data" ]; then
  cd "$MOCK" && python3 examples/automation/octos/setup_scenario.py --restore >/dev/null 2>&1 \
    && echo "restored the recorded baseline"
fi

for p in 20825 "$MOCK_PORT" "$FIDDLER_PORT"; do
  n=$(lsof -nP -iTCP:"$p" -sTCP:LISTEN 2>/dev/null | wc -l | tr -d ' ')
  echo "  :$p listeners: $n"
done
