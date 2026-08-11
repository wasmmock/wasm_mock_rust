#!/bin/bash
# Stand up the octos UI Protocol mock on :3335. No octos server needed — the
# wasm answers the client itself via the tcp_response capability.
bash cli/cli.sh set_mock_tcp_fiddler mock_octos mock_octos 20825
