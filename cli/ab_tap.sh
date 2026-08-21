#!/bin/bash
# Install the prompt tap on the MITM proxy, then open a capture window.
#
#   bash cli/ab_tap.sh install          # deploy the guest (once per change)
#   bash cli/ab_tap.sh capture [secs]   # open a window; prints a ReportId
#
# The proxy is martian on :20810 — NOT :3335, which is a TCP port-map target and
# has nothing to do with HTTP. Point the client you want to observe at :20810 and
# give it the harness CA, or its TLS handshake fails and it cannot talk at all:
#
#   curl -s http://localhost:20825/cert/pem -o /tmp/octos-harness-ca.pem
#   HTTPS_PROXY=http://127.0.0.1:20810 \
#   NODE_EXTRA_CA_CERTS=/tmp/octos-harness-ca.pem \
#     claude --dangerously-skip-permissions
#
# Prompts are recorded as report steps prefixed `AB-PROMPT:`; read them back
# with scripts/ab_replay.py, which pulls /report_data/<ReportId>.
set -u

case "${1:-}" in
  install)
    bash cli/cli.sh set_mock_fiddler ab_prompt_tap ab_prompt_tap 20825
    ;;
  capture)
    bash cli/cli.sh fiddler ab_prompt_tap ab_prompt_tap 20825 "${2:-300}"
    ;;
  *)
    echo "usage: bash cli/ab_tap.sh install | capture [seconds]" >&2
    exit 1
    ;;
esac
