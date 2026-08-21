#!/bin/bash
# Install the passthrough fiddler in front of the mock: 3340 -> 3335.
#
# Deploy order matters. This binds :3340 and forwards to :3335, so mock_octos
# has to be up first (`bash cli/mock_octos.sh`) or PoolInit dials a dead
# upstream and the guest is never invoked at all.
#
# Installing only puts the wasm in place; it records nothing until a capture
# window is opened with cli/fiddler_ws_3340.sh.
bash cli/cli.sh set_mock_tcp_fiddler fiddler_tcp_ws_3340 fiddler_ws_3340 20825 40
