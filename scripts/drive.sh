#!/bin/bash
# Deploy one mock_octos scenario, drive octoscode against it, screenshot the TUI.
#   drive.sh <scenario-name> [extra-steps]
# Screenshots land in $SHOTS as <scenario>__<step>.txt (plain) and .ans (colour).
#
# No per-shot wire log any more. The wasm fiddler only records while a capture
# window is open, so to get the exchange behind a run, start one alongside it:
#   bash cli/fiddler_ws_3340.sh 240 &   # prints a ReportId when it closes
#   bash scripts/drive.sh pristine yes
#   curl http://localhost:20825/report_data/<ReportId>
# Read that report knowing its `response` is whatever frame came next on the
# wire, not the answer to the `request` beside it — pair on the JSON-RPC id.
set -u

MOCK=/Users/alanpoon/Documents/go/wasm_mock_rust
OCTOSCODE=/Users/alanpoon/Documents/rust/robius/octoscode
SHOTS=$MOCK/screenshots/octoscode
# Which octoscode to drive. Override to test a release build, a branch build, or
# an installed binary against the same fixtures:
#   OCTOSCODE_BIN=~/.cargo/bin/octoscode bash scripts/drive.sh pristine yes
#   OCTOSCODE_BIN=$OCTOSCODE/target/release/octoscode bash scripts/drive.sh loops-100
BIN=${OCTOSCODE_BIN:-$OCTOSCODE/target/debug/octoscode}
SCEN="$1"
EXTRA="${2:-no}"
# Baked into the guests' port maps (ws_3340.rs / mock_octos.rs) and mirrored in
# cli/test_suite.xml — not a knob.
FIDDLER_PORT=3340

mkdir -p "$SHOTS"

shot() { # shot <step-name>
  tmux capture-pane -t sc -p          > "$SHOTS/${SCEN}__$1.txt"
  tmux capture-pane -t sc -p -e       > "$SHOTS/${SCEN}__$1.ans"
  echo "    shot: ${SCEN}__$1"
}

# Pending verbs the store sets at dispatch. Shooting while one is still up
# captures the request, not the reply — the interesting frame is the one after
# the response lands, so wait for it to clear rather than guessing a sleep.
PENDING='Creating loop|Pausing loop|Resuming loop|Firing loop|Deleting loop|Listing loops'

send() { # send "<text>" [settle]
  tmux send-keys -t sc "$1"; sleep 2
  tmux send-keys -t sc Escape; sleep 1
  tmux send-keys -t sc Enter; sleep 2
  for _ in $(seq 1 "${2:-12}"); do
    tmux capture-pane -t sc -p | tail -1 | grep -qE "$PENDING" || break
    sleep 1
  done
  sleep 1
}

echo "== $SCEN"

# Fail here rather than 40s later with an empty tmux pane. A missing or
# unbuilt binary otherwise shows up as "no server running on /tmp/tmux-501",
# which says nothing about the actual cause.
[ -x "$BIN" ] || { echo "    not executable: $BIN (build it, or set OCTOSCODE_BIN)"; exit 1; }

# Stamp the build next to its screenshots. `--version` carries the git short
# hash and build date, so a shot stays attributable to a commit — without it a
# sweep taken before a fix is indistinguishable from one taken after, which is
# exactly the confusion to avoid when a screenshot becomes a bug report.
BUILD=$("$BIN" --version 2>/dev/null | head -1)
echo "    binary: $BUILD"
mkdir -p "$SHOTS"
printf '%s\n%s\n' "$BIN" "$BUILD" > "$SHOTS/${SCEN}__build.txt"

cd "$MOCK" || exit 1
if [ "$SCEN" = "pristine" ]; then
  python3 examples/automation/octos/setup_scenario.py --restore >/dev/null 2>&1 || {
    echo "    restore failed"; exit 1; }
else
  python3 examples/automation/octos/setup_scenario.py "$SCEN" >/dev/null 2>&1 || {
    echo "    setup failed"; exit 1; }
fi
bash cli/mock_octos.sh >/dev/null 2>&1 || { echo "    deploy failed"; exit 1; }
sleep 2

# Reinstall the fiddler AFTER every mock redeploy, never before.
#
# cli/mock_octos.sh above rebinds :3341, which strands the 3340 fiddler's
# upstream: PoolInit dials the remote half of the port map once, at install
# time, and the goroutine that reads the client will not start until that dial
# lands. Skip this and the scenario deploys cleanly, the client connects, and
# nothing is ever answered.
bash cli/mock_ws_3340.sh >/dev/null 2>&1 || { echo "    fiddler install failed"; exit 1; }
sleep 2

tmux kill-session -t sc 2>/dev/null
tmux new-session -d -s sc -c "$OCTOSCODE" -x 200 -y 50 \
  "OCTOSCODE_NO_SPLASH=1 $BIN --profile-id alan --session 'alan:local:tui#coding' \
   --endpoint ws://127.0.0.1:$FIDDLER_PORT/api/ui-protocol/ws --auth-token local-dev-token 2>&1"

# Wait for the session to open. The footer shows the endpoint until it does,
# then swaps in the workspace path — so wait for the ws:// URL to GO, rather
# than for any particular path. Against the mock the path comes from the
# capture's `session/status/read` and reads `octos-tui`, not `octoscode`;
# matching on the latter never fired, and the 40s timeout it fell through to
# put every command past the point where the mock stops answering a connection.
for _ in $(seq 1 20); do
  sleep 2
  tmux capture-pane -t sc -p 2>/dev/null | tail -2 | head -1 | grep -q 'ui-protocol/ws' || break
done
sleep 2
shot 00-open

send '/loop list' 30
shot 01-loop-list
tmux send-keys -t sc Escape; sleep 2

if [ "$EXTRA" = "yes" ]; then
  send '/loop every 5m run tests' 30 ; shot 02-loop-create
  send '/loop pause loop_02' 30      ; shot 03-loop-pause
  send '/loop resume loop_02' 30     ; shot 04-loop-resume
  send '/loop fire-now loop_02' 30   ; shot 05-loop-fire-now
  send '/loop delete loop_02' 30     ; shot 06-loop-delete
  send '/loop list' 30               ; shot 07-loop-list-after
  tmux send-keys -t sc Escape; sleep 1
fi

tmux kill-session -t sc 2>/dev/null
echo "   done $SCEN"
