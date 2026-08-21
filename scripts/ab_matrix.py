"""A/B the comparable agent surface: drive BOTH TUIs, screenshot both, diff.

    python3 scripts/ab_matrix.py --list
    python3 scripts/ab_matrix.py --only read_file,bash_exec      # a subset
    python3 scripts/ab_matrix.py                                  # everything

Unlike ab_replay.py (headless, text only) this drives the real terminal UIs, so
what it captures is what a person would actually see: which tools ran, how the
work was rendered, and the final answer. Screenshots land in
screenshots/ab_matrix/<capability>__{claude,octoscode}.{txt,ans}.

## Why these capabilities and not "all of them"

Claude Code has a large surface octoscode has no equivalent for — /rewind,
hooks, plugins, subagents, plan mode, checkpointing. Comparing those produces a
feature-presence checklist, not an A/B: the answer is "one has it, one does
not", which you already know. The matrix below is deliberately restricted to
behaviour BOTH agents implement, where a difference in the output is a
difference in quality rather than a difference in feature set.

Each probe names the tool it is meant to exercise, so a run also shows whether
an agent reached for the expected tool or talked its way around it.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import time

REPO = "/Users/alanpoon/Documents/rust/robius/octoscode"
MOCK = "/Users/alanpoon/Documents/go/wasm_mock_rust"
SHOTS = os.path.join(MOCK, "screenshots", "ab_matrix")
OUT = os.path.join(MOCK, "ab_results", "matrix.json")

OCTOSCODE_BIN = os.environ.get("OCTOSCODE_BIN", f"{REPO}/target/debug/octoscode")
OCTOS_PORT = os.environ.get("OCTOS_PORT", "3334")
PROXY = os.environ.get("AB_PROXY", "http://127.0.0.1:20810")
CA = os.environ.get("AB_CA", "/tmp/octos-harness-ca.pem")

# (id, expected tool, prompt)
MATRIX = [
    ("read_file",    "Read",      "Read Cargo.toml in this repo and tell me the package version and edition."),
    ("glob_search",  "Glob",      "How many .rs files are under src/ in this repo? Just the number and how you counted."),
    ("grep_search",  "Grep",      "Where is APPUI_METHOD_LOOP_CREATE defined? Give file and line."),
    ("bash_exec",    "Bash",      "Run `git log -1 --oneline` here and show me the actual output."),
    ("write_file",   "Write",     "Create /tmp/ab_matrix_probe.txt containing exactly the single word: hello"),
    ("edit_file",    "Edit",      "In /tmp/ab_matrix_probe.txt, change the word hello to goodbye. Show the file after."),
    ("multi_step",   "Bash+Write","Write a script that prints the 3 largest .rs files under src/ by line count, run it, show real output."),
    ("web_search",   "WebSearch", "What is the current latest stable Rust version and its release date?"),
    ("web_fetch",    "WebFetch",  "Fetch https://example.com and tell me the page title."),
    ("uncertainty",  "(none)",    "What does the --turbo-borrow flag do in cargo 1.95?"),
    ("ambiguity",    "(none)",    "Fix the timeout bug."),
    ("error_report", "Bash",      "Run `cargo build --target definitely-not-a-target` and explain exactly why it failed."),
    ("python_script","Write+Bash","Write a Python script that prints the 5 most common file extensions in this repo with counts, run it, and show the real output."),
]

# octoscode-only: its slash commands have no Claude Code equivalent, so these are
# probed on one side and reported as capability presence rather than an A/B.
OCTOSCODE_COMMANDS = [
    ("cmd_help",   "/help"),
    ("cmd_loop",   "/loop list"),
    ("cmd_agents", "/agents"),
    ("cmd_status", "/status"),
    ("cmd_model",  "/model"),
]

# Terminal chrome that is not part of an answer.
# Rendered while a turn is still RUNNING, and only then.
#
# Each TUI has exactly one trustworthy in-progress marker:
#   Claude Code : the footer gains `esc to interrupt`
#   octoscode   : the status reads `Working (Ns)`
#
# Matching the spinner glyph instead does not work. Claude keeps a `✻ <verb>
# for Ns` line AFTER the turn ends, and it rotates the verb — Cooked, Brewed,
# Churned, Bloviating, Ran, Worked. A pattern that excluded two of them treated
# every finished Claude turn as still busy, so each probe burned its entire
# timeout instead of ~4 seconds.
BUSY = re.compile(r"esc to interrupt|state [◜◝◞◟◡◠] Working|Working \(\d+")

NOISE = re.compile(r"(Composer|Ask Octos|bypass permissions|tmux focus-events|"
                   r"^\s*[─│┌└╭╰]|^\s*$|Enter send|Tab agents|Ctrl\+O expand|"
                   r"for shortcuts|⏵⏵)")


def sh(cmd, timeout=120):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout)


def pane(session):
    r = sh(f"tmux capture-pane -t {session} -p")
    return r.stdout if r.returncode == 0 else ""


def start_claude(session):
    sh(f"tmux kill-session -t {session} 2>/dev/null")
    sh(f'tmux new-session -d -s {session} -c {REPO} -x 200 -y 50 '
       f'"HTTPS_PROXY={PROXY} HTTP_PROXY={PROXY} NODE_EXTRA_CA_CERTS={CA} '
       f'claude --dangerously-skip-permissions"')
    time.sleep(14)
    return "❯" in pane(session) or "for shortcuts" in pane(session)


def restart_octos_serve():
    """Bounce `octos serve` and wait for it to listen again.

    Needed because of octos issue #2086: the server intermittently stops
    completing WebSocket upgrades while still listening. octoscode gives up
    after a 3s connect timeout — tighter than this harness's own client, which
    retries — so the TUI is the first thing to fail and a long run dies on
    probe 1. Restarting is the only known recovery.
    """
    sh("pkill -f 'octos serve --auth-token local-dev-token --port 3334'")
    time.sleep(5)
    subprocess.Popen(
        f"nohup {REPO}/octos serve --auth-token local-dev-token --port {OCTOS_PORT} "
        f"--data-dir ~/.octos-tui-data > /tmp/octos-harness/matrix-serve.log 2>&1",
        shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        stdin=subprocess.DEVNULL)
    time.sleep(20)


def start_octoscode(session, attempts=3):
    for attempt in range(attempts):
        if _try_start_octoscode(session):
            return True
        print(f"     octoscode TUI failed (attempt {attempt + 1}/{attempts}); "
              f"restarting octos serve — see octos#2086")
        restart_octos_serve()
    return False


def _try_start_octoscode(session):
    sh(f"tmux kill-session -t {session} 2>/dev/null")
    sh(f'tmux new-session -d -s {session} -c {REPO} -x 200 -y 50 '
       f'"OCTOSCODE_NO_SPLASH=1 {OCTOSCODE_BIN} --profile-id alan '
       f"--session 'alan:local:tui#coding' "
       f'--endpoint ws://127.0.0.1:{OCTOS_PORT}/api/ui-protocol/ws '
       f'--auth-token local-dev-token"')
    for _ in range(15):
        time.sleep(2)
        text = pane(session)
        if "failed to connect" in text or "connect timed out" in text:
            return False          # fail fast; the retry loop will restart serve
        if "Ask Octos" in text:
            return True
    return False


def send_and_settle(session, prompt, quiet_for=8, cap=300):
    """Type, submit, then wait for the pane to stop changing.

    Stability beats status-string matching: the two TUIs word "done"
    differently ("✓ Done" vs returning to the prompt), and a matcher tuned to
    one of them silently truncates the other's answer mid-stream.
    """
    # Clear the composer BEFORE typing. Claude Code pre-fills a suggested
    # next prompt ("what dependencies does it use?") after a turn; typing on top
    # of it sends the suggestion concatenated with the probe, so the next probe
    # silently measures the wrong question.
    sh(f"tmux send-keys -t {session} C-u")
    time.sleep(1)
    sh(f"tmux send-keys -t {session} {json.dumps(prompt)}")
    time.sleep(2)
    sh(f"tmux send-keys -t {session} Enter")
    last, stable_since, start = None, time.time(), time.time()
    while time.time() - start < cap:
        time.sleep(2)
        now = pane(session)
        busy = bool(BUSY.search(now))
        if now != last or busy:
            last, stable_since = now, time.time()
        elif time.time() - stable_since >= quiet_for:
            return True
    return False


def answer_from(pane_text, prompt):
    """Best-effort extraction of what the agent said after the prompt echo."""
    lines = pane_text.splitlines()
    head = prompt[:40]
    start = 0
    for i, l in enumerate(lines):
        if head[:30] in l:
            start = i + 1
    body = [l for l in lines[start:] if not NOISE.search(l)]
    return "\n".join(body).strip()


def tools_used(pane_text):
    """Tool invocations visible in the transcript, by either TUI's rendering.

    The two render differently and a single pattern misses one of them:
    octoscode prints `Read(path: "…")`, while Claude Code COLLAPSES repeats to
    `Read 1 file (ctrl+o to expand)`. Matching only the call form reported
    Claude as having used no tools on a probe where it plainly had.
    """
    found = set()
    aliases = {
        "Read": [r"Read\(", r"\bRead \d+ file"],
        "Write": [r"Write\(", r"\bWrote? \d+ (file|line)"],
        "Edit": [r"Edit\(", r"\bUpdated? \d+ file", r"\bEdited\b"],
        "Bash": [r"Bash\(", r"\$ cmd:", r"\bRan \d+ command"],
        "Glob": [r"Glob\(", r"\bFound \d+ file"],
        "Grep": [r"Grep\(", r"\bSearched \d+ "],
        "WebSearch": [r"WebSearch\(", r"\bweb_search\b", r"\bSearched the web\b"],
        "WebFetch": [r"WebFetch\(", r"\bweb_fetch\b", r"\bFetch\(" ],
    }
    for name, pats in aliases.items():
        if any(re.search(pat, pane_text) for pat in pats):
            found.add(name)
    return sorted(found)


def shot(session, cap_id, agent):
    os.makedirs(SHOTS, exist_ok=True)
    base = os.path.join(SHOTS, f"{cap_id}__{agent}")
    sh(f"tmux capture-pane -t {session} -p    > {json.dumps(base + '.txt')}")
    sh(f"tmux capture-pane -t {session} -p -e > {json.dumps(base + '.ans')}")
    return base + ".txt"


def untracked_snapshot():
    """Untracked paths in the repo right now.

    The probes make the agents write real files (ext_stats.py, scratch scripts),
    and a benchmark that leaves debris in a working tree is a benchmark people
    stop running. Diffing untracked-before against untracked-after removes
    exactly what this run created and nothing a person left there earlier —
    safer than deleting a hardcoded list, which would miss scripts the agents
    chose to name differently.
    """
    r = sh(f"git -C {REPO} status --porcelain --untracked-files=all")
    return {l[3:] for l in r.stdout.splitlines() if l.startswith("?? ")}


def cleanup(before, extra_paths):
    created = untracked_snapshot() - before
    removed = []
    for rel in sorted(created):
        path = os.path.join(REPO, rel)
        if os.path.isfile(path):
            os.remove(path); removed.append(rel)
    for path in extra_paths:
        if os.path.isfile(path):
            os.remove(path); removed.append(path)
    return removed


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", help="comma-separated capability ids")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--cap", type=int, default=300, help="per-probe seconds")
    ap.add_argument("--no-cleanup", action="store_true",
                    help="keep files the probes created (default: remove them)")
    ap.add_argument("--commands", action="store_true",
                    help="also probe octoscode's slash commands (octoscode only)")
    args = ap.parse_args()

    if args.list:
        for cid, tool, prompt in MATRIX:
            print(f"  {cid:14} [{tool:10}] {prompt[:70]}")
        return

    probes = MATRIX
    if args.only:
        want = {x.strip() for x in args.only.split(",")}
        probes = [p for p in MATRIX if p[0] in want]
        missing = want - {p[0] for p in probes}
        if missing:
            raise SystemExit(f"unknown capability id(s): {sorted(missing)}")

    before = untracked_snapshot()
    print(f"{len(probes)} capability probe(s); both agents driven via TUI")
    if not start_claude("abc"):
        raise SystemExit("claude TUI did not come up")
    print("  claude TUI up")
    if not start_octoscode("abo"):
        raise SystemExit("octoscode TUI did not come up (is octos serve on "
                         f"{OCTOS_PORT}? see octos issue #2086)")
    print("  octoscode TUI up")

    rows = []
    for cid, tool, prompt in probes:
        print(f"\n[{cid}] expects {tool}")
        row = {"id": cid, "expected_tool": tool, "prompt": prompt}
        for agent, session in (("claude", "abc"), ("octoscode", "abo")):
            done = send_and_settle(session, prompt, cap=args.cap)
            text = pane(session)
            row[agent] = {
                "settled": done,
                "answer": answer_from(text, prompt)[-1200:],
                "tools": tools_used(text),
                "shot": shot(session, cid, agent),
            }
            print(f"     {agent:10}: {'ok' if done else 'TIMEOUT':7} "
                  f"tools={row[agent]['tools']} ({len(row[agent]['answer'])} chars)")
        rows.append(row)
        os.makedirs(os.path.dirname(OUT), exist_ok=True)
        with open(OUT, "w", encoding="utf-8") as fh:
            json.dump(rows, fh, indent=2)

    # octoscode slash commands. Reported separately and never diffed: Claude
    # Code's slash commands are a different set entirely, so pairing them would
    # invent a comparison that does not exist.
    if args.commands:
        print("\n-- octoscode slash commands (no Claude equivalent, not an A/B)")
        cmd_rows = []
        for cid, cmd in OCTOSCODE_COMMANDS:
            # A slash command opens a menu; Escape dismisses it, then Enter
            # submits. Same dance drive.sh uses.
            sh("tmux send-keys -t abo C-u"); time.sleep(1)
            sh(f"tmux send-keys -t abo {json.dumps(cmd)}"); time.sleep(2)
            sh("tmux send-keys -t abo Escape"); time.sleep(1)
            sh("tmux send-keys -t abo Enter"); time.sleep(8)
            text = pane("abo")
            status = text.splitlines()[-1] if text.splitlines() else ""
            unavailable = "unavailable" in status or "not advertised" in status
            cmd_rows.append({"id": cid, "command": cmd,
                             "status": status.strip()[:140],
                             "available": not unavailable,
                             "shot": shot("abo", cid, "octoscode")})
            print(f"   {cmd:12} {'ok' if not unavailable else 'UNAVAILABLE'}")
            sh("tmux send-keys -t abo Escape"); time.sleep(1)
        with open(os.path.join(MOCK, "ab_results", "octoscode_commands.json"),
                  "w", encoding="utf-8") as fh:
            json.dump(cmd_rows, fh, indent=2)

    if not args.no_cleanup:
        removed = cleanup(before, ["/tmp/ab_matrix_probe.txt"])
        print(f"\ncleaned up {len(removed)} file(s) the probes created: "
              f"{removed if removed else '(none)'}")

    print(f"\n== {len(rows)} probes -> {OUT}")
    print(f"   screenshots -> {SHOTS}")
    for r in rows:
        c, o = r["claude"], r["octoscode"]
        flag = ""
        if not o["settled"] or not o["answer"]:
            flag = "  <-- octoscode produced nothing"
        elif c["tools"] != o["tools"]:
            flag = f"  <-- different tools: {c['tools']} vs {o['tools']}"
        print(f"   {r['id']:14}{flag}")


if __name__ == "__main__":
    main()
