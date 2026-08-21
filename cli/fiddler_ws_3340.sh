#!/bin/bash
# Open a capture window on the 3340 -> 3335 fiddler and print its ReportId.
#
#   bash cli/fiddler_ws_3340.sh [seconds]
#
# Blocks for the duration, then returns {"Header":{"ReportId":"..."}}. Fetch the
# recorded exchange with:
#   curl http://localhost:20825/report_data/<ReportId>
#   python3 scripts/pair_capture.py <(...)        # or read it directly
#
# The default is 120s, not the 20s cli/fiddler_ws.sh uses: 20 seconds is not
# enough to launch the TUI, wait for session/open and drive a command, and a
# window that closes mid-run records the setup rather than the thing you wanted.
#
# Two properties of this recorder to keep in mind when reading the report:
#   - `response` is NOT the answer to `request`. Rows pair a request with
#     whatever frame came next on that connection, which is often an unrelated
#     notification. Only the JSON-RPC `id` identifies a reply.
#   - Payloads the host cannot frame-align (anything split across its 20000-byte
#     reads) are forwarded intact but never recorded, so a large reply such as
#     session/open can be missing from the report while the client received it
#     perfectly well.
bash cli/cli.sh tcp_fiddler fiddler_tcp_ws_3340 fiddler_ws_3340 20825 "${1:-120}"
