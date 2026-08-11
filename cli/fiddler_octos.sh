#!/bin/bash
# Record a report while the octos mock serves. set_mock_tcp_fiddler (mock_octos.sh)
# installs the mock but never opens a report — only the fiddler/automation
# operations register a report uid, so this is what makes report_data non-empty.
bash cli/cli.sh tcp_fiddler mock_octos mock_octos 20825 "${1:-20}"
