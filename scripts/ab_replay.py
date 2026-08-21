"""Replay captured prompts at octoscode and at Claude Code, and diff the two.

    python3 scripts/ab_replay.py --report <ReportId>       # replay a capture
    python3 scripts/ab_replay.py --prompts prompts.txt     # or a hand-written set
    python3 scripts/ab_replay.py --report <id> --dry-run   # show what would run

Reads prompts the wasm tap recorded (`AB-PROMPT:` steps in the fiddler report),
sends each to BOTH agents, and writes `ab_results/<n>.json` plus a summary.

Two things this deliberately does:

**One prompt at a time, on your say-so.** Nothing fires while you work; a replay
costs one DeepSeek turn and one Claude turn per prompt, and that should be a
decision rather than a side effect of having a terminal open.

**Handicap the comparison honestly.** octoscode is driven by deepseek here, so
`--context` prepends the framing in CONTEXT_PREFIX below. That makes the
comparison "octoscode with a fair prompt vs Claude Code" rather than a straight
model-vs-model race, which is the useful question when the thing under test is
the CLIENT. It is off by default so the raw gap is visible too — run both and
the delta between them is what the framing bought.
"""
import argparse
import json
import os
import subprocess
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

MOCK_SERVER = "http://localhost:20825"
# `SaveReport` (util/fs.go) writes `report/<uid>.json` relative to the mock
# server's cwd. That file holds the RAW recorded request — body, headers, and
# the `x-api-key` that went with it — none of which the guest can prevent,
# because the host records the request itself. Extract the prompts, then delete
# it; see scrub_report.
REPORT_DIR = os.environ.get(
    "OCTOS_MOCK_REPORT_DIR",
    "/Users/alanpoon/Documents/go/wasm_mock_server/report",
)
OUT_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ab_results")

# Prepended to the octoscode side when --context is passed.
#
# Not a jailbreak or a hint about the answer — it states the operating context a
# coding agent needs and that Claude Code gets from its own system prompt:
# where it is, what it may assume, and what a good answer looks like. Without
# it the two sides are asked the same words in different situations, and the
# result measures the framing rather than the client.
CONTEXT_PREFIX = """You are a coding agent working in a real repository on the user's machine.
Before answering: state your assumptions if the request is ambiguous, prefer
concrete file paths and commands over generalities, and say plainly when you do
not know something rather than guessing. If the task needs multiple steps, do
them in order and report what actually happened, including failures.

Task:
"""


def prompts_from_report(report_id):
    """Pull `AB-PROMPT:` steps out of a fiddler report.

    Fetched with curl rather than urllib: python's stdlib HTTP is blocked in
    this sandbox (it fails the same way `websockets` did against octos serve),
    while curl goes straight through. One less thing that can be the reason a
    capture looks empty.
    """
    url = f"{MOCK_SERVER}/report_data/{report_id}"
    out = subprocess.run(["curl", "-s", "-m", "30", url],
                         capture_output=True, text=True)
    if out.returncode != 0 or not out.stdout.strip():
        raise SystemExit(f"could not fetch {url}: {out.stderr.strip() or 'empty'}")
    data = json.loads(out.stdout)
    body = data.get("response_body") or {}
    found, misses = [], 0
    for test in body.get("tests") or []:
        for step in test.get("Step") or []:
            desc = step.get("description") or ""
            if desc.startswith("AB-PROMPT: "):
                found.append(desc[len("AB-PROMPT: "):])
            elif desc.startswith("AB-PROMPT-MISS"):
                misses += 1
    if misses:
        print(f"note: {misses} request(s) had no readable user text", file=sys.stderr)
    # Consecutive duplicates are the same turn seen on a retry, not two asks.
    deduped = [p for i, p in enumerate(found) if i == 0 or p != found[i - 1]]
    return deduped


def scrub_report(report_id):
    """Delete the on-disk report once its prompts have been lifted out.

    The tap is careful to record only one user message, but that care governs
    only the step it adds. The host writes the whole intercepted request beside
    it — full conversation, headers, api key — so leaving the file behind
    defeats the point of filtering at all. Deleting it is the only lever
    available from outside the Go server.

    Returns a short status for the caller to print, because a scrub that
    silently did nothing is worse than no scrub: it reads as safety that is not
    there.
    """
    path = os.path.join(REPORT_DIR, f"{report_id}.json")
    if not os.path.exists(path):
        return f"nothing to scrub at {path}"
    size = os.path.getsize(path)
    try:
        os.remove(path)
    except OSError as e:
        return f"COULD NOT SCRUB {path}: {e}"
    return f"scrubbed {path} ({size} bytes)"


# Both agents must run in the SAME repo or the comparison is unfair — and
# `claude -p` reads .claude/settings.local.json from its cwd, so running it from
# this repo silently ignored the WebSearch/WebFetch grants added to octoscode's.
# That is why Claude reported "permission not granted" on every web prompt.
AGENT_CWD = "/Users/alanpoon/Documents/rust/robius/octoscode"


def ask_claude(prompt, timeout=300):
    """Headless Claude Code. Its own system prompt supplies the context."""
    try:
        r = subprocess.run(["claude", "-p", prompt], capture_output=True,
                           text=True, timeout=timeout, cwd=AGENT_CWD)
        return {"ok": r.returncode == 0,
                "text": (r.stdout or r.stderr).strip(),
                "error": None if r.returncode == 0 else f"exit {r.returncode}"}
    except FileNotFoundError:
        return {"ok": False, "text": "", "error": "claude not on PATH"}
    except subprocess.TimeoutExpired:
        return {"ok": False, "text": "", "error": f"timed out after {timeout}s"}


def ask_octoscode(prompt, port, session, timeout=300):
    """Drive octoscode's backend over the UI Protocol and collect the reply."""
    from octos_client import Client
    profile = session.split(":", 1)[0]
    try:
        c = Client(port=port, session=session, profile=profile, cwd=AGENT_CWD)
    except Exception as e:
        return {"ok": False, "text": "", "error": f"connect: {e}"}
    try:
        if not c.open():
            return {"ok": False, "text": "", "error": "session/open got no reply"}
        # turn_id MUST be a UUID. Anything else is refused with
        #   -32602 invalid params for turn/start: UUID parsing failed
        # and `ab-<timestamp>` ids meant every turn was rejected outright.
        turn_id = str(uuid.uuid4())
        # And CALL, not send: fire-and-forget hid that rejection completely —
        # the driver waited out its timeout for notifications that were never
        # coming and reported `ok` with an empty answer, which reads as
        # "octoscode returned nothing" rather than "the request was invalid".
        ack = c.call("turn/start", {"session_id": c.session, "turn_id": turn_id,
                                    "input": [{"kind": "text", "text": prompt}]},
                     timeout=30)
        if ack is None:
            return {"ok": False, "text": "", "error": "turn/start got no reply"}
        if "error" in ack:
            return {"ok": False, "text": "",
                    "error": f"turn/start rejected: {json.dumps(ack['error'])[:200]}"}
        # The answer arrives as pushed notifications, and almost all of them are
        # wrapped: a real turn is 36 `projection/envelope` frames carrying a
        # typed `payload`, not the flat `message/delta` notifications the
        # capability list advertises. Reading the flat names is what made every
        # octoscode reply come back as 0 chars while the TUI showed a perfectly
        # good answer. Shapes taken from the recorded stream in
        # examples/automation/octos/turn_stream.json:
        #
        #   payload.type = assistant_delta      data.text  (streamed piece)
        #   payload.type = assistant_persisted  data.text  (the complete reply)
        #   payload.type = reasoning_delta      data.text  (thinking - NOT the answer)
        #   payload.type = turn_terminal        data.outcome
        deltas, final, reasoning = [], None, []
        end = time.time() + timeout
        while time.time() < end:
            try:
                raw = c.ws.recv(2)
            except Exception:
                continue
            try:
                msg = json.loads(raw)
            except Exception:
                continue
            method = msg.get("method") or ""
            params = msg.get("params") or {}

            if method == "projection/envelope":
                payload = params.get("payload") or {}
                ptype = payload.get("type")
                data = payload.get("data") or {}
                if ptype == "assistant_delta":
                    deltas.append(data.get("text") or "")
                elif ptype == "assistant_persisted":
                    # The whole reply, and authoritative: the recorded capture is
                    # missing deltas the fiddler could not frame-align, so the
                    # concatenated stream reads clipped where this does not.
                    final = data.get("text") or final
                elif ptype == "reasoning_delta":
                    reasoning.append(data.get("text") or "")
                elif ptype == "turn_terminal":
                    outcome = data.get("outcome")
                    if outcome and outcome != "completed":
                        return {"ok": False, "text": final or "".join(deltas),
                                "error": f"turn_terminal outcome={outcome}",
                                "reasoning_chars": len("".join(reasoning))}
                    break
            elif method == "turn/error":
                return {"ok": False, "text": final or "".join(deltas),
                        "error": json.dumps(params)[:200],
                        "reasoning_chars": len("".join(reasoning))}
            elif method == "turn/completed":
                break

        text = (final if final is not None else "".join(deltas)).strip()
        return {"ok": True, "text": text, "error": None,
                "reasoning_chars": len("".join(reasoning))}
    finally:
        c.ws.close()


def compare(a, b):
    """Cheap, honest signals. Judging quality is a human's job, not a word count."""
    at, bt = a.get("text") or "", b.get("text") or ""
    return {
        "claude_chars": len(at),
        "octoscode_chars": len(bt),
        "claude_ok": a.get("ok"),
        "octoscode_ok": b.get("ok"),
        "octoscode_empty": bool(at) and not bt,
        "only_claude_answered": bool(at) and not bt,
        "only_octoscode_answered": bool(bt) and not at,
    }


def main():
    ap = argparse.ArgumentParser()
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--report", help="ReportId from cli/ab_tap.sh capture")
    src.add_argument("--prompts", help="file with one prompt per line")
    ap.add_argument("--port", type=int, default=3334,
                    help="octoscode backend port (3334 real octos serve, 3341 mock)")
    # The session key's profile segment MUST match --profile: the server rejects
    # a mismatch with `profile_id does not match session_id profile`, which
    # looks like a connection problem in the results table but is a bad default.
    ap.add_argument("--profile", default="alan")
    ap.add_argument("--session", default=None,
                    help="defaults to <profile>:local:tui#bench")
    ap.add_argument("--context", action="store_true",
                    help="prepend CONTEXT_PREFIX to the octoscode side")
    ap.add_argument("--limit", type=int, default=0, help="only the first N prompts")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--keep-report", action="store_true",
                    help="do NOT delete the on-disk report after extracting "
                         "prompts (it contains the raw request and api key)")
    args = ap.parse_args()

    if args.report:
        prompts = prompts_from_report(args.report)
        # Scrub before replaying, not after: a replay can fail or be
        # interrupted, and the raw request should not outlive the extraction
        # either way.
        if args.keep_report:
            print("WARNING: --keep-report; raw request + api key stay on disk")
        else:
            print(scrub_report(args.report))
    else:
        with open(args.prompts, encoding="utf-8") as fh:
            prompts = [l.strip() for l in fh if l.strip() and not l.startswith("#")]
    if args.limit:
        prompts = prompts[:args.limit]

    session = args.session or f"{args.profile}:local:tui#bench"
    print(f"{len(prompts)} prompt(s); context prefix {'ON' if args.context else 'off'}; "
          f"session {session}")
    if args.dry_run:
        for i, p in enumerate(prompts):
            print(f"  [{i}] {p[:120]}")
        print("\ndry run: nothing sent, nothing spent")
        return

    os.makedirs(OUT_DIR, exist_ok=True)
    rows = []
    for i, prompt in enumerate(prompts):
        octos_prompt = (CONTEXT_PREFIX + prompt) if args.context else prompt
        print(f"\n[{i}] {prompt[:100]}")
        a = ask_claude(prompt)
        print(f"     claude    : {'ok' if a['ok'] else a['error']} ({len(a['text'])} chars)")
        b = ask_octoscode(octos_prompt, args.port, session)
        print(f"     octoscode : {'ok' if b['ok'] else b['error']} ({len(b['text'])} chars)")
        row = {"index": i, "prompt": prompt, "context_prefix": args.context,
               "claude": a, "octoscode": b, "signals": compare(a, b)}
        rows.append(row)
        with open(os.path.join(OUT_DIR, f"{i:03d}.json"), "w", encoding="utf-8") as fh:
            json.dump(row, fh, indent=2)

    summary = os.path.join(OUT_DIR, "summary.json")
    with open(summary, "w", encoding="utf-8") as fh:
        json.dump(rows, fh, indent=2)

    failed = [r for r in rows if not r["octoscode"]["ok"]]
    empty = [r for r in rows if r["signals"]["octoscode_empty"]]
    print(f"\n== {len(rows)} compared -> {summary}")
    print(f"   octoscode errored : {len(failed)}")
    print(f"   octoscode empty   : {len(empty)}")
    if failed or empty:
        print("   ^ these are the candidates worth reading before filing anything")


if __name__ == "__main__":
    main()
