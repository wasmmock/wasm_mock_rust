"""Scan screenshots/octoscode/*.txt for renderings worth a human look.

    python3 scripts/triage_shots.py [screenshot-dir]

Flags are heuristics, not verdicts. Two of them exist because of how the shots
are taken rather than what the client did, and they are reported separately so a
timing artefact never gets filed as a defect:

- `pending`  the status line still shows the verb the store sets at DISPATCH
             ("Creating loop", "Listing loops"). Usually the shot beat the
             reply — re-run that scenario with a longer settle before believing
             it. Only a verb that survives a generous settle is interesting.
- `decode`   the client said it could not decode a result. That one is real:
             it means the fixture and the client's struct disagree.
"""
import os
import re
import sys

PENDING = re.compile(r"\b(Creating loop|Pausing loop|Resuming loop|Firing loop|"
                     r"Deleting loop|Listing loops)\b")
DECODE = re.compile(r"invalid_result|failed to decode|malformed_frame")
ERROR = re.compile(r"state x Error|is unavailable:|not advertised|replay_lossy")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f]")


def status_of(lines):
    for line in reversed(lines):
        if "state" in line and "|" in line:
            return line.strip()
    return ""


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "screenshots/octoscode"
    rows = []
    for name in sorted(os.listdir(root)):
        if not name.endswith(".txt"):
            continue
        text = open(os.path.join(root, name), encoding="utf-8", errors="replace").read()
        status = status_of(text.splitlines())
        flags = []
        if DECODE.search(text):
            flags.append("decode")
        if ERROR.search(text):
            flags.append("error")
        if CONTROL.search(text):
            flags.append("control-chars")
        if PENDING.search(status):
            flags.append("pending")
        if flags:
            rows.append((name, flags, status[:150]))

    real = [r for r in rows if [f for f in r[1] if f != "pending"]]
    timing = [r for r in rows if not [f for f in r[1] if f != "pending"]]

    print(f"== worth reading ({len(real)})")
    for name, flags, status in real:
        print(f"  {name}\n    flags: {','.join(flags)}\n    {status}")
    print(f"\n== pending-only, probably shot too early ({len(timing)})")
    for name, _, status in timing:
        print(f"  {name}\n    {status}")


if __name__ == "__main__":
    main()
