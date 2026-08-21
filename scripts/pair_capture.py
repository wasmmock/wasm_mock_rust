"""Pair a wstap.py capture into request/response exchanges, by JSON-RPC id.

    python3 scripts/pair_capture.py capture.jsonl [method-prefix] [-o out.json]

`method-prefix` filters to one family (`loop/`, `session/`, …); omit it for all.
With `-o` the paired exchanges plus every matching notification are written as
JSON, which is the form `examples/automation/octos/captures/` keeps.

Why this exists: the wasm tcp_fiddler's report pairs a request with whatever
frame happened to follow it on the wire, so its `response` field is routinely a
`server/heartbeat` or an unrelated notification. Pairing on `id` is the only
thing that actually identifies a reply — and it also makes "this method got no
answer at all" a fact you can read off rather than a guess.
"""
import json
import sys


def main():
    args = [a for a in sys.argv[1:]]
    out_path = None
    if "-o" in args:
        i = args.index("-o")
        out_path = args[i + 1]
        del args[i:i + 2]
    if not args:
        raise SystemExit(__doc__)
    path, prefix = args[0], (args[1] if len(args) > 1 else "")

    recs = [json.loads(line) for line in open(path, encoding="utf-8")]
    requests = {
        r["id"]: r for r in recs
        if r["dir"] == "c2s" and (r.get("method") or "").startswith(prefix)
        and r.get("id") is not None
    }
    replies = {r["id"]: r for r in recs if r["dir"] == "s2c" and r.get("id") in requests}
    notifications = [
        json.loads(r["raw"]) for r in recs
        if r["dir"] == "s2c" and r.get("kind") == "notification"
        and (r.get("method") or "").startswith(prefix)
    ]

    exchanges = []
    for rid, req in requests.items():
        reply = replies.get(rid)
        exchanges.append({
            "method": req["method"],
            "request": json.loads(req["raw"]),
            "response": json.loads(reply["raw"]) if reply else None,
        })

    answered = sorted({e["method"] for e in exchanges if e["response"]})
    silent = sorted({e["method"] for e in exchanges if not e["response"]})
    for e in exchanges:
        print("###", e["method"], "" if e["response"] else "  <NO RESPONSE>")
        print("  REQ ", json.dumps(e["request"])[:300])
        if e["response"]:
            print("  RES ", json.dumps(e["response"])[:400])
    print()
    print("answered      :", answered)
    print("no response   :", [m for m in silent if m not in answered])
    print("notifications :", sorted({n.get("method") for n in notifications}))

    if out_path:
        json.dump({"source": path, "exchanges": exchanges,
                   "notifications": notifications},
                  open(out_path, "w", encoding="utf-8"), indent=2)
        print("wrote", out_path)


if __name__ == "__main__":
    main()
