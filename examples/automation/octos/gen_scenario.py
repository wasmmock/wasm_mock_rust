"""Generate 200 scenarios by mutating the octos mock's recorded responses.

Each scenario is a directory of overriding JSON files; the driver copies the
pristine set, drops the overrides on top, rebuilds and deploys. Mutations stay
valid JSON on purpose — the target is the CLIENT's decode/render paths, not
build.rs's validator.
"""
import copy, json, os, shutil

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pristine")
# Overridable, because the default is a scratchpad belonging to one session and
# main() starts by deleting whatever is at it. Point OCTOS_SCENARIO_OUT
# somewhere else rather than editing this line when the session changes.
OUT = os.environ.get(
    "OCTOS_SCENARIO_OUT",
    "/private/tmp/claude-501/-Users-alanpoon-Documents-go-wasm-mock-rust/660649f3-42c8-4d7a-8552-e2d77d81b7b2/scratchpad/scenarios",
)

FILES = {
    "cap": "config_capabilities_list.json",
    "llm": "profile_llm_list.json",
    "status": "session_status_read.json",
    "hydrate": "session_hydrate.json",
    "agents": "agent_list.json",
    "goal": "session_goal_get.json",
    "loops": "loop_list.json",
    "turn": "turn_stream.json",
    "heartbeat": "server_heartbeat.json",
}
try:
    BASE = {k: json.load(open(os.path.join(SRC, v), encoding="utf-8")) for k, v in FILES.items()}
except FileNotFoundError as error:
    # The mutations are all diffs against the recorded baseline, so there is
    # nothing this module can do without one. The baseline is a copy of the data
    # dir taken while it is still untouched — see setup_scenario.py, which is
    # also what puts a scenario back.
    raise SystemExit(f"{error.filename}: no baseline. Run `python3 setup_scenario.py --seed` first.")

SCENARIOS = []


def scenario(name, area):
    def wrap(fn):
        SCENARIOS.append((name, area, fn))
        return fn
    return wrap


def base(key):
    return copy.deepcopy(BASE[key])


# Nasty-but-legal strings a real transcript can genuinely contain.
ANSI = "[31mRED[0m [2J[H"
CTRL = "bell back vtab formfeed nul-ish"
RTL = "‮holle‬ العربية עברית"
ZWJ = "\U0001f469‍\U0001f4bb \U0001f3f4\U000e0067\U000e0062\U000e0073\U000e0063\U000e0074\U000e007f ́́́"
WIDE = "你好世界 ＡＢＣ ㄱㄴㄷ"
NEWLINES = "\n".join(f"line {i}" for i in range(400))


def msg(seq, role="user", content="hello", thread="019fea57-d50c-7d91-88ab-12649bfcff8f", **extra):
    m = {
        "seq": seq, "role": role, "content": content, "thread_id": thread,
        "persisted_at": "2026-08-10T06:24:12.686828Z",
        "message_id": f"alan:local:tui#coding:{seq}:178634305268682800{seq % 10}",
    }
    m.update(extra)
    return m


# ---------------------------------------------------------------- hydrate ---
for name, mutate in [
    ("hydrate-no-messages", lambda h: h.update(messages=[])),
    ("hydrate-one-user-message", lambda h: h.update(messages=[msg(0)])),
    ("hydrate-one-assistant-message", lambda h: h.update(messages=[msg(0, "assistant")])),
    ("hydrate-empty-content", lambda h: h.update(messages=[msg(0, content="")])),
    ("hydrate-whitespace-content", lambda h: h.update(messages=[msg(0, content="   \t  ")])),
    ("hydrate-unknown-role", lambda h: h.update(messages=[msg(0, role="oracle")])),
    ("hydrate-role-system", lambda h: h.update(messages=[msg(0, role="system")])),
    ("hydrate-role-empty", lambda h: h.update(messages=[msg(0, role="")])),
    ("hydrate-ansi-content", lambda h: h.update(messages=[msg(0, content=ANSI)])),
    ("hydrate-control-chars", lambda h: h.update(messages=[msg(0, content=CTRL)])),
    ("hydrate-rtl-content", lambda h: h.update(messages=[msg(0, content=RTL)])),
    ("hydrate-zwj-emoji", lambda h: h.update(messages=[msg(0, content=ZWJ)])),
    ("hydrate-fullwidth-cjk", lambda h: h.update(messages=[msg(0, content=WIDE)])),
    ("hydrate-many-newlines", lambda h: h.update(messages=[msg(0, content=NEWLINES)])),
    ("hydrate-huge-content", lambda h: h.update(messages=[msg(0, content="x" * 200_000)])),
    ("hydrate-no-trailing-newline-huge-word", lambda h: h.update(messages=[msg(0, content="y" * 20_000)])),
    ("hydrate-tabs-only", lambda h: h.update(messages=[msg(0, content="\t\t\t\t")])),
    ("hydrate-cr-only-linebreaks", lambda h: h.update(messages=[msg(0, content="a\rb\rc")])),
    ("hydrate-null-byte-escape", lambda h: h.update(messages=[msg(0, content="ab")])),
    ("hydrate-500-messages", lambda h: h.update(messages=[msg(i, "user" if i % 2 == 0 else "assistant") for i in range(500)])),
    ("hydrate-duplicate-seq", lambda h: h.update(messages=[msg(0), msg(0, "assistant")])),
    ("hydrate-reversed-seq", lambda h: h.update(messages=[msg(5), msg(4, "assistant"), msg(3)])),
    ("hydrate-negative-seq", lambda h: h.update(messages=[msg(-3)])),
    ("hydrate-huge-seq", lambda h: h.update(messages=[msg(9_007_199_254_740_991)])),
    ("hydrate-seq-gap", lambda h: h.update(messages=[msg(0), msg(900, "assistant")])),
    ("hydrate-orphan-thread-id", lambda h: h.update(messages=[msg(0, thread="does-not-exist")])),
    ("hydrate-threads-empty", lambda h: h.update(threads=[])),
    ("hydrate-thread-empty-seqs", lambda h: h.update(threads=[{"thread_id": "t1", "root_seq": 0, "message_seqs": [], "status": "unknown"}])),
    ("hydrate-thread-seqs-missing-messages", lambda h: h.update(threads=[{"thread_id": "t1", "root_seq": 0, "message_seqs": [99, 100], "status": "unknown"}])),
    ("hydrate-thread-status-active", lambda h: [t.update(status="active") for t in h["threads"]]),
    ("hydrate-turns-empty", lambda h: h.update(turns=[])),
    ("hydrate-turn-running", lambda h: [t.update(state="running") for t in h["turns"]]),
    ("hydrate-turn-failed", lambda h: [t.update(state="failed") for t in h["turns"]]),
    ("hydrate-turn-unknown-state", lambda h: [t.update(state="wat") for t in h["turns"]]),
    ("hydrate-turn-no-completed-at", lambda h: [t.pop("completed_at", None) for t in h["turns"]]),
    ("hydrate-no-cursor", lambda h: h.pop("cursor", None)),
    ("hydrate-cursor-seq-zero", lambda h: h["cursor"].update(seq=0)),
    ("hydrate-cursor-seq-huge", lambda h: h["cursor"].update(seq=9_007_199_254_740_991)),
    ("hydrate-cursor-stream-empty", lambda h: h["cursor"].update(stream="")),
    ("hydrate-no-context", lambda h: h.pop("context", None)),
    ("hydrate-no-context-state", lambda h: h.pop("context_state", None)),
    ("hydrate-recovery-lossy", lambda h: h["context_state"].update(recovery_state="lossy")),
    ("hydrate-recovery-unknown", lambda h: h["context_state"].update(recovery_state="???")),
    ("hydrate-token-estimate-huge", lambda h: h["context_state"].update(token_estimate=9_000_000)),
    ("hydrate-item-count-zero", lambda h: h["context_state"].update(item_count=0)),
    ("hydrate-generation-zero", lambda h: h["context_state"].update(generation=0)),
    ("hydrate-pending-approval", lambda h: h.update(pending_approvals=[{"approval_id": "a1", "kind": "command", "session_id": h["session_id"]}])),
    ("hydrate-pending-question", lambda h: h.update(pending_questions=[{"question_id": "q1", "prompt": "pick one", "options": []}])),
    ("hydrate-no-tool-envelopes", lambda h: h.update(replayed_tool_envelopes=[])),
    ("hydrate-unknown-tool-payload", lambda h: [e["payload"].update(type="tool_unknown") for e in h["replayed_tool_envelopes"]]),
    ("hydrate-tool-envelope-no-data", lambda h: [e["payload"].pop("data", None) for e in h["replayed_tool_envelopes"]]),
    ("hydrate-replayed-envelopes-populated", lambda h: h.update(replayed_envelopes=[{"seq": 1, "payload": {"type": "assistant_delta", "data": {"text": "x"}}}])),
    ("hydrate-no-reasoning", lambda h: [m.pop("reasoning_content", None) for m in h["messages"]]),
    ("hydrate-huge-reasoning", lambda h: h.update(messages=[msg(0, "assistant", reasoning_content="r" * 100_000)])),
    ("hydrate-empty-reasoning", lambda h: h.update(messages=[msg(0, "assistant", reasoning_content="")])),
    ("hydrate-no-message-id", lambda h: [m.pop("message_id", None) for m in h["messages"]]),
    ("hydrate-no-persisted-at", lambda h: [m.pop("persisted_at", None) for m in h["messages"]]),
    ("hydrate-bad-timestamp", lambda h: [m.update(persisted_at="not-a-date") for m in h["messages"]]),
    ("hydrate-future-timestamp", lambda h: [m.update(persisted_at="2999-12-31T23:59:59.999999Z") for m in h["messages"]]),
    ("hydrate-tool-role-message", lambda h: h.update(messages=[msg(0, "tool", content="tool output")])),
    ("hydrate-session-id-mismatch", lambda h: h.update(session_id="someone:else:tui#coding")),
]:
    scenario(name, "hydrate")(lambda mutate=mutate: (lambda h=base("hydrate"): (mutate(h), {"hydrate": h})[1])())

# ----------------------------------------------------------- capabilities ---
CAP_METHODS = ["session/open", "turn/start", "session/hydrate", "agent/list", "loop/list",
               "session/goal/get", "mcp/status/list", "tool/status/list", "session/status/read",
               "profile/llm/list", "launch/resolve", "turn/interrupt", "session/compact"]
for method in CAP_METHODS:
    def make(method=method):
        c = base("cap")
        c["capabilities"]["supported_methods"] = [m for m in c["capabilities"]["supported_methods"] if m != method]
        return {"cap": c}
    scenario(f"cap-drop-method-{method.replace('/', '-')}", "capabilities")(make)

CAP_FEATURES = ["projection.envelope.v2", "state.session_hydrate.v1", "coding.autonomy.v1",
                "plan.todos.v1", "user_question.v1", "context.lifecycle.v1",
                "session.workspace_cwd.v1", "approval.typed.v1", "runtime.policy_stamp.v1",
                "coding.tool_contract.v1", "harness.task_control.v1"]
for feat in CAP_FEATURES:
    def make(feat=feat):
        c = base("cap")
        c["capabilities"]["supported_features"] = [f for f in c["capabilities"]["supported_features"] if f != feat]
        return {"cap": c}
    scenario(f"cap-drop-feature-{feat}", "capabilities")(make)

for name, mutate in [
    ("cap-no-methods", lambda c: c["capabilities"].update(supported_methods=[])),
    ("cap-no-notifications", lambda c: c["capabilities"].update(supported_notifications=[])),
    ("cap-no-features", lambda c: c["capabilities"].update(supported_features=[])),
    ("cap-drop-projection-notification", lambda c: c["capabilities"].update(
        supported_notifications=[n for n in c["capabilities"]["supported_notifications"] if n != "projection/envelope"])),
    ("cap-unknown-feature", lambda c: c["capabilities"]["supported_features"].append("from.the.future.v9")),
    ("cap-unknown-method", lambda c: c["capabilities"]["supported_methods"].append("quantum/entangle")),
    ("cap-schema-version-999", lambda c: c["capabilities"].update(capabilities_schema_version=999)),
    ("cap-schema-version-zero", lambda c: c["capabilities"].update(capabilities_schema_version=0)),
    ("cap-no-version-block", lambda c: c["capabilities"].pop("version", None)),
    ("cap-protocol-mismatch", lambda c: c["capabilities"]["version"].update(protocol="octos-ui/v99")),
    ("cap-jsonrpc-1", lambda c: c["capabilities"]["version"].update(jsonrpc="1.0")),
    ("cap-empty-object", lambda c: c.update(capabilities={})),
]:
    scenario(name, "capabilities")(lambda mutate=mutate: (lambda c=base("cap"): (mutate(c), {"cap": c})[1])())

# ------------------------------------------------------------- turn stream ---
def envelopes(stream):
    return [e for e in stream if e.get("method") == "projection/envelope"]


def renumber(stream):
    seq = 0
    for e in stream:
        if e.get("method") == "projection/envelope":
            seq += 1
            e["params"]["seq"] = seq
    return stream


def payload_of(e):
    return e.get("params", {}).get("payload", {})


def delta(seq, text, kind="assistant_delta"):
    return {"jsonrpc": "2.0", "method": "projection/envelope", "params": {
        "session_id": "alan:local:tui", "topic": "coding",
        "thread_id": "019febbb-e207-7630-83e3-b3038fef300e", "seq": seq,
        "cursor": {"stream": "alan:local:tui#coding~cwd-74427c29e62e0989", "seq": 1600 + seq},
        "turn_id": "019febbb-e207-7630-83e3-b3038fef300e",
        "payload": {"type": kind, "data": {"text": text, "assistant_segment_id": "019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}}


def terminal_only():
    s = base("turn")
    return [e for e in s if payload_of(e).get("type") == "turn_terminal"]


for name, mutate in [
    ("turn-empty-stream", lambda s: []),
    ("turn-terminal-only", lambda s: terminal_only()),
    ("turn-no-terminal", lambda s: [e for e in s if payload_of(e).get("type") != "turn_terminal"]),
    ("turn-two-terminals", lambda s: s + terminal_only()),
    ("turn-outcome-error", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].update(outcome="error") or True]),
    ("turn-outcome-cancelled", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].update(outcome="cancelled") or True]),
    ("turn-outcome-interrupted", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].update(outcome="interrupted") or True]),
    ("turn-outcome-unknown", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].update(outcome="??") or True]),
    ("turn-no-token-usage", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].pop("token_usage", None) or True]),
    ("turn-negative-tokens", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].update(token_usage={"input_tokens": -5, "output_tokens": -9}) or True]),
    ("turn-huge-tokens", lambda s: [e for e in s if not payload_of(e).get("type") == "turn_terminal" or payload_of(e)["data"].update(token_usage={"input_tokens": 9_007_199_254_740_991, "output_tokens": 9_007_199_254_740_991}) or True]),
    ("turn-no-assistant-persisted", lambda s: [e for e in s if payload_of(e).get("type") != "assistant_persisted"]),
    ("turn-no-user-message", lambda s: [e for e in s if payload_of(e).get("type") != "user_message"]),
    ("turn-no-deltas", lambda s: [e for e in s if payload_of(e).get("type") not in ("assistant_delta", "reasoning_delta")]),
    ("turn-no-reasoning-deltas", lambda s: [e for e in s if payload_of(e).get("type") != "reasoning_delta"]),
    ("turn-empty-delta-text", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text="") or True]),
    ("turn-ansi-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text=ANSI) or True]),
    ("turn-control-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text=CTRL) or True]),
    ("turn-rtl-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text=RTL) or True]),
    ("turn-zwj-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text=ZWJ) or True]),
    ("turn-wide-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text=WIDE) or True]),
    ("turn-huge-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text="z" * 60_000) or True]),
    ("turn-newline-delta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].update(text=NEWLINES) or True]),
    ("turn-2000-deltas", lambda s: [s[0]] + [delta(i + 1, f"w{i} ") for i in range(2000)] + terminal_only()),
    ("turn-duplicate-seq", lambda s: [e for e in s if not e.get("params", {}).get("seq") or e["params"].update(seq=1) or True]),
    ("turn-seq-gaps", lambda s: [e for e in s if not e.get("params", {}).get("seq") or e["params"].update(seq=e["params"]["seq"] * 7) or True]),
    ("turn-seq-from-zero", lambda s: [e for e in s if not e.get("params", {}).get("seq") or e["params"].update(seq=e["params"]["seq"] - 1) or True]),
    ("turn-seq-negative", lambda s: [e for e in s if not e.get("params", {}).get("seq") or e["params"].update(seq=-e["params"]["seq"]) or True]),
    ("turn-seq-reversed", lambda s: list(reversed(s))),
    ("turn-cursor-seq-frozen", lambda s: [e for e in s if not e.get("params", {}).get("cursor") or e["params"]["cursor"].update(seq=1) or True]),
    ("turn-no-cursor", lambda s: [e for e in s if not e.get("params", {}).get("cursor") or e["params"].pop("cursor", None) or True]),
    ("turn-no-thread-id", lambda s: [e for e in s if "thread_id" not in e.get("params", {}) or e["params"].pop("thread_id", None) or True]),
    ("turn-no-turn-id", lambda s: [e for e in s if "turn_id" not in e.get("params", {}) or e["params"].pop("turn_id", None) or True]),
    ("turn-no-topic", lambda s: [e for e in s if "topic" not in e.get("params", {}) or e["params"].pop("topic", None) or True]),
    ("turn-other-topic", lambda s: [e for e in s if "topic" not in e.get("params", {}) or e["params"].update(topic="review") or True]),
    ("turn-full-session-in-envelope", lambda s: [e for e in s if "topic" not in e.get("params", {}) or e["params"].update(session_id="alan:local:tui#coding") or True]),
    ("turn-unknown-payload-type", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e).update(type="hologram_delta") or True]),
    ("turn-payload-no-data", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e).pop("data", None) or True]),
    ("turn-no-segment-id", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_delta" or payload_of(e)["data"].pop("assistant_segment_id", None) or True]),
    ("turn-mismatched-segment-id", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_persisted" or payload_of(e)["data"].update(assistant_segment_id="other:assistant:9") or True]),
    ("turn-persisted-empty-text", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_persisted" or payload_of(e)["data"].update(text="") or True]),
    ("turn-persisted-huge-text", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_persisted" or payload_of(e)["data"].update(text="q" * 150_000) or True]),
    ("turn-persisted-ansi", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_persisted" or payload_of(e)["data"].update(text=ANSI) or True]),
    ("turn-persisted-no-meta", lambda s: [e for e in s if not payload_of(e).get("type") == "assistant_persisted" or payload_of(e)["data"].pop("meta", None) or True]),
    ("turn-reasoning-after-answer", lambda s: renumber([e for e in s if payload_of(e).get("type") != "reasoning_delta"] + [delta(999, "late thought", "reasoning_delta")])),
    ("turn-progress-unknown-kind", lambda s: [e for e in s if e.get("method") != "progress/updated" or e["params"]["metadata"].update(kind="teleporting") or True]),
    ("turn-progress-no-metadata", lambda s: [e for e in s if e.get("method") != "progress/updated" or e["params"].pop("metadata", None) or True]),
    ("turn-token-cost-nulls", lambda s: [e for e in s if not (e.get("method") == "progress/updated" and e["params"].get("metadata", {}).get("kind") == "token_cost_update") or e["params"]["metadata"].update(token_cost={"input_tokens": None, "output_tokens": None, "session_cost": None, "model": None, "context_window": None}) or True]),
    ("turn-orchestration-never-idle", lambda s: [e for e in s if not (e.get("method") == "session/orchestration" and e["params"].get("active") is False)]),
    ("turn-normalization-no-state", lambda s: [e for e in s if e.get("method") != "context/normalization_reported" or e["params"].pop("context_state", None) or True]),
    ("turn-only-notifications-no-envelopes", lambda s: [e for e in s if e.get("method") != "projection/envelope"]),
    ("turn-user-message-empty", lambda s: [e for e in s if not payload_of(e).get("type") == "user_message" or payload_of(e)["data"].update(text="") or True]),
]:
    scenario(name, "turn")(lambda mutate=mutate: (lambda s=base("turn"): {"turn": mutate(s) if mutate(base("turn")) is not None else s})())

# ------------------------------------------------------------------ loops ---
for name, mutate in [
    ("loops-empty", lambda l: l.update(loops=[])),
    ("loops-100", lambda l: l.update(loops=[dict(l["loops"][0], loop_id=f"loop_{i:03}") for i in range(100)])),
    ("loops-status-running", lambda l: [x.update(status="running") for x in l["loops"]]),
    ("loops-status-expired", lambda l: [x.update(status="expired") for x in l["loops"]]),
    ("loops-status-unknown", lambda l: [x.update(status="schroedinger") for x in l["loops"]]),
    ("loops-interval-set", lambda l: [x.update(interval_seconds=300, mode="interval") for x in l["loops"]]),
    ("loops-interval-zero", lambda l: [x.update(interval_seconds=0) for x in l["loops"]]),
    ("loops-interval-negative", lambda l: [x.update(interval_seconds=-60) for x in l["loops"]]),
    ("loops-empty-prompt", lambda l: [x.update(prompt="") for x in l["loops"]]),
    ("loops-huge-prompt", lambda l: [x.update(prompt="p" * 40_000) for x in l["loops"]]),
    ("loops-ansi-prompt", lambda l: [x.update(prompt=ANSI) for x in l["loops"]]),
    ("loops-newline-prompt", lambda l: [x.update(prompt=NEWLINES) for x in l["loops"]]),
    ("loops-zero-timestamps", lambda l: [x.update(next_run_at_ms=0, expires_at_ms=0, created_at_ms=0, updated_at_ms=0) for x in l["loops"]]),
    ("loops-negative-timestamps", lambda l: [x.update(next_run_at_ms=-1, expires_at_ms=-1) for x in l["loops"]]),
    ("loops-huge-timestamps", lambda l: [x.update(next_run_at_ms=9_007_199_254_740_991, expires_at_ms=9_007_199_254_740_991) for x in l["loops"]]),
    ("loops-no-loop-id", lambda l: [x.pop("loop_id", None) for x in l["loops"]]),
    ("loops-last-run-set", lambda l: [x.update(last_run_at_ms=1784275555650) for x in l["loops"]]),
    ("loops-session-mismatch", lambda l: [x.update(session_id="other:local:tui#coding") for x in l["loops"]]),
]:
    scenario(name, "loops")(lambda mutate=mutate: (lambda l=base("loops"): (mutate(l), {"loops": l})[1])())

# -------------------------------------------------------------- profile llm ---
for name, mutate in [
    ("llm-primary-null", lambda p: p.update(primary=None, llm={"primary": None, "fallbacks": []})),
    ("llm-no-fallbacks", lambda p: p.update(fallbacks=[], llm={"primary": p["primary"], "fallbacks": []})),
    ("llm-many-fallbacks", lambda p: p.update(fallbacks=[dict(p["fallbacks"][0], model=f"m{i}") for i in range(60)])),
    ("llm-no-api-key", lambda p: (p["primary"].update(has_api_key=False, available=False), [f.update(has_api_key=False, available=False) for f in p["fallbacks"]])),
    ("llm-route-null", lambda p: p["primary"].update(route=None)),
    ("llm-base-url-set", lambda p: p["primary"].update(base_url="http://localhost:9999/v1")),
    ("llm-empty-model", lambda p: p["primary"].update(model="", model_id="")),
    ("llm-huge-model-name", lambda p: p["primary"].update(model="m" * 5_000)),
    ("llm-unknown-provider", lambda p: p["primary"].update(provider="acme-ai", family_id="acme-ai")),
    ("llm-nothing-selected", lambda p: p["primary"].update(selected=False)),
    ("llm-no-runtime-stamp", lambda p: p.pop("runtime_policy_stamp", None)),
    ("llm-stamp-network-unknown", lambda p: p["runtime_policy_stamp"].update(network="maybe")),
    ("llm-stamp-no-runtime-mode", lambda p: p["runtime_policy_stamp"].pop("runtime_mode", None)),
    ("llm-profile-id-mismatch", lambda p: p.update(profile_id="somebody-else")),
]:
    scenario(name, "llm")(lambda mutate=mutate: (lambda p=base("llm"): (mutate(p), {"llm": p})[1])())

# ------------------------------------------------------------------ status ---
for name, mutate in [
    ("status-health-degraded", lambda s: s["health"].update(status="degraded")),
    ("status-health-error", lambda s: s["health"].update(status="error", detail="backend on fire")),
    ("status-health-unknown", lambda s: s["health"].update(status="???")),
    ("status-no-health", lambda s: s.pop("health", None)),
    ("status-usage-populated", lambda s: s.update(usage={"input_tokens": 711050, "output_tokens": 44435, "session_cost": 0.2126})),
    ("status-cursor-unhealthy", lambda s: s["cursor"].update(healthy=False)),
    ("status-no-replay", lambda s: s["cursor"].update(replay_supported=False)),
    ("status-no-model", lambda s: s.pop("model", None)),
    ("status-model-unselected", lambda s: s["model"].update(selected=False)),
    ("status-mcp-nonzero", lambda s: s["mcp_summary"].update(connected=3, failed=2, connecting=1, disabled=4)),
    ("status-tools-nonzero", lambda s: s["tool_summary"].update(visible=42, enabled=40, denied=2)),
    ("status-permission-unknown", lambda s: s.update(permission_profile="omniscient")),
    ("status-sandbox-unknown", lambda s: s.update(sandbox="none-at-all")),
    ("status-no-capabilities", lambda s: s.pop("capabilities", None)),
    ("status-limits-zero", lambda s: s["runtime_policy_stamp"].update(max_agents_per_session=0, max_loops_per_session=0, max_agent_tree_depth=0)),
    ("status-limits-negative", lambda s: s["runtime_policy_stamp"].update(max_agents_per_session=-1, loop_min_interval_seconds=-1)),
    ("status-budget-zero", lambda s: s["runtime_policy_stamp"].update(goal_default_token_budget=0, goal_max_token_budget=0)),
    ("status-workspace-null", lambda s: s["runtime_policy_stamp"].update(workspace_root=None)),
    ("status-no-context-state", lambda s: s.pop("context_state", None)),
    ("status-session-mismatch", lambda s: s.update(session_id="mismatch:local:tui#coding")),
]:
    scenario(name, "status")(lambda mutate=mutate: (lambda s=base("status"): (mutate(s), {"status": s})[1])())

# ------------------------------------------------------------ agents / goal ---
def agent(i, state="running"):
    return {"agent_id": f"agent_{i}", "session_id": "alan:local:tui#coding", "profile_id": "alan",
            "role": "explorer", "state": state, "created_at_ms": 1784275348509,
            "objective": "look around", "depth": 1}


for name, mutate in [
    ("agents-one-running", lambda a: a.update(agents=[agent(1)])),
    ("agents-many", lambda a: a.update(agents=[agent(i) for i in range(40)])),
    ("agents-unknown-state", lambda a: a.update(agents=[agent(1, "vibing")])),
    ("agents-no-agent-id", lambda a: a.update(agents=[{k: v for k, v in agent(1).items() if k != "agent_id"}])),
    ("agents-huge-objective", lambda a: a.update(agents=[dict(agent(1), objective="o" * 50_000)])),
    ("agents-deep-tree", lambda a: a.update(agents=[dict(agent(i), depth=i) for i in range(1, 12)])),
]:
    scenario(name, "agents")(lambda mutate=mutate: (lambda a=base("agents"): (mutate(a), {"agents": a})[1])())

for name, mutate in [
    ("goal-set", lambda g: g.update(goal={"objective": "ship it", "token_budget": 2_000_000, "spent": 10, "status": "active"})),
    ("goal-budget-exhausted", lambda g: g.update(goal={"objective": "ship it", "token_budget": 10, "spent": 10, "status": "exhausted"})),
    ("goal-huge-objective", lambda g: g.update(goal={"objective": "g" * 60_000, "token_budget": 1, "status": "active"})),
    ("goal-null-fields", lambda g: g.update(goal={"objective": None, "token_budget": None, "status": None})),
    ("goal-unknown-status", lambda g: g.update(goal={"objective": "x", "status": "quantum"})),
]:
    scenario(name, "goal")(lambda mutate=mutate: (lambda g=base("goal"): (mutate(g), {"goal": g})[1])())


# ------------------------------------------------- cross-payload additions ---
# APPENDED, not filed with the other `hydrate` cases, and it must stay that way:
# a scenario's position IS its scenario_id, and that id is what a campaign's
# results.jsonl and report_index.tsv are keyed on. Inserting this at case 62
# would silently renumber the 150 cases after it and orphan every result already
# recorded against them. `area` still says "hydrate", so grouping is unaffected.
def recovery_rebuilt():
    """`recovery_state` "rebuilt" instead of "exact", on every payload carrying it.

    Not a hydrate-only edit on purpose. The client applies `context_state` from
    `session/hydrate`, from `session/status/read` AND from the turn stream's
    opening `context/normalization_reported` envelope, each overwriting the last.
    Changing one would leave the session's health decided by call ordering.

    "rebuilt" is a value the real server actually emits — `ContextRecoveryState`
    serialises to exactly `exact | rebuilt | unknown` — so this is a state that
    can happen in production, not an invented string.
    """
    hydrate, status, turn = base("hydrate"), base("status"), base("turn")
    for body in (hydrate, status):
        body["context"]["state"]["recovery_state"] = "rebuilt"
        body["context_state"]["recovery_state"] = "rebuilt"
    for envelope in turn:
        state = envelope.get("params", {}).get("context_state")
        if state:
            state["recovery_state"] = "rebuilt"
    return {"hydrate": hydrate, "status": status, "turn": turn}


scenario("hydrate-recovery-rebuilt", "hydrate")(recovery_rebuilt)


# ----------------------------------------------------------- expectations ---
# What the CLIENT should do when served each scenario, in one English sentence.
#
# These are not assertions the rig can check — nothing here drives the TUI's
# screen. They are carried into the wasm on the heartbeat and emitted as a
# report step, so a human reading a run's report sees what the case was probing
# next to what the client actually did. They are recorded as FAILING steps on
# purpose: a passing step is scrolled past, and an unverified claim has no
# business being coloured green.
EXPECTATIONS = {
    # ------------------------------------------------------------ hydrate ---
    "hydrate-no-messages": "session/hydrate returns no messages: the transcript opens empty and the client still reaches a usable prompt.",
    "hydrate-one-user-message": "One user message hydrates: exactly one user row, and no assistant reply invented to pair with it.",
    "hydrate-one-assistant-message": "One assistant message hydrates: it renders on its own, with no preceding user turn faked in.",
    "hydrate-empty-content": "A message with empty content: rendered as a blank row or deliberately skipped, never collapsing the rest of the transcript.",
    "hydrate-whitespace-content": "Content of spaces and tabs only: kept as a blank row rather than trimmed into a missing message.",
    "hydrate-unknown-role": "Role 'oracle' is unknown: rendered generically; hydrate must not abort over an unrecognised role.",
    "hydrate-role-system": "A system-role message: shown as system/meta or hidden on purpose — never attributed to the user.",
    "hydrate-role-empty": "An empty role string: treated as unknown rather than decoded into a panic.",
    "hydrate-ansi-content": "ANSI escapes in transcript content: sanitised before painting, so replayed history cannot clear or recolour the TUI.",
    "hydrate-control-chars": "Bell, backspace, vertical tab and form feed in content: neutralised so the terminal is not driven by transcript data.",
    "hydrate-rtl-content": "Right-to-left text with a bidi override: rendered without the override leaking into the surrounding chrome.",
    "hydrate-zwj-emoji": "ZWJ emoji and stacked combining marks: measured as grapheme clusters, so nothing is split mid-cluster.",
    "hydrate-fullwidth-cjk": "Full-width CJK and Hangul: laid out on cell width, with no wrap through a double-width cell.",
    "hydrate-many-newlines": "A 400-line message: scrolled in the transcript rather than truncated or allowed to block the prompt.",
    "hydrate-huge-content": "A 200 KB message: hydrate completes and the client stays responsive.",
    "hydrate-no-trailing-newline-huge-word": "A 20 000-character unbroken word: hard-wrapped to the viewport instead of overflowing one line.",
    "hydrate-tabs-only": "Content of tabs only: expanded to a visible blank row.",
    "hydrate-cr-only-linebreaks": "Bare CR line breaks: treated as line breaks, not as a carriage return painting over text already drawn.",
    "hydrate-null-byte-escape": "An escaped null inside content: decoded and rendered without terminating the string early.",
    "hydrate-500-messages": "500 hydrated messages: all load, scrollback stays navigable, and startup does not hang.",
    "hydrate-duplicate-seq": "Two messages sharing seq 0: both kept, or one de-duplicated deliberately — not one silently overwriting the other.",
    "hydrate-reversed-seq": "Messages arriving in descending seq: ordered by seq, not by position in the array.",
    "hydrate-negative-seq": "A negative seq: HydratedMessage.seq is a u64, so the whole result fails to decode — expect a visible `invalid_result` error naming session/hydrate, and a session still usable without its history.",
    "hydrate-huge-seq": "seq at 2^53-1: no overflow, and no false replay-loss warning from the cursor tracker.",
    "hydrate-seq-gap": "A gap between seq 0 and seq 900: backfilled or flagged, never rendered as loss of the whole transcript.",
    "hydrate-orphan-thread-id": "A message pointing at a thread that does not exist: shown under a fallback thread rather than dropped.",
    "hydrate-threads-empty": "No threads at all: messages still render under a default thread.",
    "hydrate-thread-empty-seqs": "A thread listing no message seqs: rendered as an empty thread, not as a broken index.",
    "hydrate-thread-seqs-missing-messages": "A thread pointing at seqs that were never sent: the dangling references are ignored, not resolved into empty rows.",
    "hydrate-thread-status-active": "Every thread marked active: shown as live without the client waiting forever for one to settle.",
    "hydrate-turns-empty": "No turns in hydrate: the client is idle and ready to prompt.",
    "hydrate-turn-running": "A turn hydrated as running: shown in progress, and the client still recovers to idle.",
    "hydrate-turn-failed": "A turn hydrated as failed: shown as failed, with the prompt still usable.",
    "hydrate-turn-unknown-state": "Turn state 'wat': treated as unknown rather than failing the hydrate decode.",
    "hydrate-turn-no-completed-at": "A finished turn with no completed_at: rendered without a timestamp.",
    "hydrate-no-cursor": "hydrate with no cursor: `cursor` is a required field, so the whole result fails to decode — expect a visible `invalid_result` error rather than a client that quietly starts from nothing.",
    "hydrate-cursor-seq-zero": "cursor.seq 0: resumed from the beginning rather than reported as replay loss.",
    "hydrate-cursor-seq-huge": "cursor.seq at 2^53-1: later envelopes numbered lower must not be read as a catastrophic gap.",
    "hydrate-cursor-stream-empty": "An empty cursor stream name: the client either still subscribes or reports one clear error.",
    "hydrate-no-context": "hydrate with no context block: the context indicator degrades to unknown.",
    "hydrate-no-context-state": "hydrate with no context_state: the context meter is hidden rather than drawn from nothing.",
    "hydrate-recovery-lossy": "recovery_state 'lossy': decoded and stored but never drawn — summary_line() is called only from tests — so expect NO visible change. This case proves the context-health surface is missing, not that a warning appears.",
    "hydrate-recovery-unknown": "recovery_state '???': accepted as a plain string, so expect no decode failure and, like every other value, nothing on screen.",
    "hydrate-token-estimate-huge": "A 9 000 000-token estimate: the context meter clamps instead of overflowing its gauge.",
    "hydrate-item-count-zero": "item_count 0 alongside real messages: the inconsistency does not zero out the transcript.",
    "hydrate-generation-zero": "Context generation 0: accepted as a real generation, not read as 'never initialised'.",
    "hydrate-pending-approval": "A pending approval hydrates: the approval is surfaced and can be answered.",
    "hydrate-pending-question": "A pending question with no options: surfaced without leaving the client stuck on an unanswerable prompt.",
    "hydrate-no-tool-envelopes": "No replayed tool envelopes: the tool surface is empty and the transcript still renders.",
    "hydrate-unknown-tool-payload": "Tool envelope typed 'tool_unknown': skipped or shown generically, never fatal to hydrate.",
    "hydrate-tool-envelope-no-data": "A tool envelope with no data: ignored rather than unwrapped into a panic.",
    "hydrate-replayed-envelopes-populated": "replayed_envelopes carrying an assistant delta: replayed without duplicating the hydrated messages.",
    "hydrate-no-reasoning": "Messages with reasoning_content removed: no reasoning surface is drawn.",
    "hydrate-huge-reasoning": "100 KB of reasoning on one message: collapsed or scrollable, not painted whole at startup.",
    "hydrate-empty-reasoning": "Empty reasoning_content: no empty reasoning block is drawn.",
    "hydrate-no-message-id": "Messages with no message_id: still rendered, and still addressable by seq.",
    "hydrate-no-persisted-at": "Messages with no persisted_at: the field is required, so one missing timestamp fails the entire hydrate decode — expect a visible `invalid_result` error, not a row drawn without a time.",
    "hydrate-bad-timestamp": "persisted_at 'not-a-date': DateTime parsing fails and takes the whole result with it — expect a visible `invalid_result` error naming session/hydrate.",
    "hydrate-future-timestamp": "A year-2999 timestamp: formatted without overflow and without a negative 'ago'.",
    "hydrate-tool-role-message": "A 'tool' role message: rendered as tool output rather than as an assistant answer.",
    "hydrate-session-id-mismatch": "hydrate answers for a different session_id: rejected or reconciled, never shown as this session's history.",
    # ------------------------------------------------------- capabilities ---
    "cap-drop-method-session-open": "session/open is not advertised: the client says it cannot open a session rather than calling it blind and hanging.",
    "cap-drop-method-turn-start": "turn/start is not advertised: submitting a prompt is refused with a clear message rather than hanging on a turn that will never start.",
    "cap-drop-method-session-hydrate": "session/hydrate is not advertised: the client starts with an empty transcript instead of blocking startup.",
    "cap-drop-method-agent-list": "agent/list is not advertised: the agents surface is hidden, and startup continues.",
    "cap-drop-method-loop-list": "loop/list is not advertised: the loops surface is hidden, and startup continues.",
    "cap-drop-method-session-goal-get": "session/goal/get is not advertised: no goal is shown, and startup continues.",
    "cap-drop-method-mcp-status-list": "mcp/status/list is not advertised: the MCP summary is omitted rather than inferred from the counts session/status/read still carries.",
    "cap-drop-method-tool-status-list": "tool/status/list is not advertised: the tool summary is omitted, and startup continues.",
    "cap-drop-method-session-status-read": "session/status/read is not advertised: health and model chrome degrade to unknown rather than blocking the session.",
    "cap-drop-method-profile-llm-list": "profile/llm/list is not advertised: the model selector reports itself unavailable.",
    "cap-drop-method-launch-resolve": "launch/resolve is not advertised: the client falls back to its own launch decision rather than stalling before the first screen.",
    "cap-drop-method-turn-interrupt": "turn/interrupt is not advertised: the interrupt key is disabled or says it cannot cancel — not silently ignored.",
    "cap-drop-method-session-compact": "session/compact is not advertised: compaction reports itself unavailable.",
    "cap-drop-feature-projection.envelope.v2": "projection.envelope.v2 is not advertised: the client either still handles the envelopes it is sent or states plainly that streaming is off.",
    "cap-drop-feature-state.session_hydrate.v1": "state.session_hydrate.v1 is not advertised: the client starts without replayed history rather than failing.",
    "cap-drop-feature-coding.autonomy.v1": "coding.autonomy.v1 is not advertised: autonomy controls are hidden and the session still runs.",
    "cap-drop-feature-plan.todos.v1": "plan.todos.v1 is not advertised: the plan/todo surface is hidden.",
    "cap-drop-feature-user_question.v1": "user_question.v1 is not advertised: no questions are expected, and one arriving anyway must not crash the client.",
    "cap-drop-feature-context.lifecycle.v1": "context.lifecycle.v1 is not advertised: the context meter is hidden rather than shown empty.",
    "cap-drop-feature-session.workspace_cwd.v1": "session.workspace_cwd.v1 is not advertised: the cwd indicator is omitted.",
    "cap-drop-feature-approval.typed.v1": "approval.typed.v1 is not advertised: typed approvals are not offered, and one arriving anyway is still answerable or clearly refused.",
    "cap-drop-feature-runtime.policy_stamp.v1": "runtime.policy_stamp.v1 is not advertised: policy limits read as unknown, not as zero.",
    "cap-drop-feature-coding.tool_contract.v1": "coding.tool_contract.v1 is not advertised: the tool-contract check is skipped rather than reported incomplete.",
    "cap-drop-feature-harness.task_control.v1": "harness.task_control.v1 is not advertised: task-control keys are disabled.",
    "cap-no-methods": "supported_methods is empty: the client refuses with a clear message rather than calling every method blind.",
    "cap-no-notifications": "supported_notifications is empty: pushed envelopes still arrive, so the client must render them or say why it will not.",
    "cap-no-features": "supported_features is empty: every optional surface degrades off and the core session still opens.",
    "cap-drop-projection-notification": "projection/envelope is not advertised: the turn stream still arrives, and the client must not discard the whole answer.",
    "cap-unknown-feature": "An unknown feature 'from.the.future.v9': ignored rather than decoded into an error.",
    "cap-unknown-method": "An unknown method 'quantum/entangle': ignored.",
    "cap-schema-version-999": "capabilities_schema_version 999: the client warns about a newer server rather than parsing on regardless.",
    "cap-schema-version-zero": "capabilities_schema_version 0: treated as too old, with a clear message.",
    "cap-no-version-block": "No version block: protocol and jsonrpc versions are unknown, and the client says so rather than assuming a match.",
    "cap-protocol-mismatch": "protocol 'octos-ui/v99': reported as an incompatible server rather than proceeding as if matched.",
    "cap-jsonrpc-1": "jsonrpc declared '1.0': flagged as a mismatch, while the client keeps speaking 2.0 on the wire.",
    "cap-empty-object": "capabilities is an empty object: the client degrades wholesale rather than panicking on a missing key.",
    # --------------------------------------------------------------- turn ---
    "turn-empty-stream": "turn/start is accepted but nothing is pushed: the client must time out or show the turn as never delivered, not spin forever.",
    "turn-terminal-only": "Only the terminal envelope arrives: the turn completes with no answer text and the prompt returns to idle.",
    "turn-no-terminal": "The stream never terminates: the turn stays running and must remain interruptible rather than wedged.",
    "turn-two-terminals": "Two terminal envelopes for one turn: the second is ignored, not counted as another turn.",
    "turn-outcome-error": "Terminal outcome 'error': the turn is shown as failed and the prompt becomes usable again.",
    "turn-outcome-cancelled": "Terminal outcome 'cancelled': shown as cancelled, with whatever streamed kept in the transcript.",
    "turn-outcome-interrupted": "Terminal outcome 'interrupted': shown as interrupted, with the partial answer retained.",
    "turn-outcome-unknown": "Terminal outcome '??': treated as an unknown ending that still returns the client to idle.",
    "turn-no-token-usage": "Terminal with no token_usage: the usage line is omitted rather than shown as zero.",
    "turn-negative-tokens": "Negative token counts: clamped or shown raw, never underflowing the session total.",
    "turn-huge-tokens": "Token counts at 2^53-1: formatted without overflow and without breaking the cost line.",
    "turn-no-assistant-persisted": "No assistant_persisted envelope: the streamed deltas stand as the final answer rather than being cleared.",
    "turn-no-user-message": "No user_message envelope: the prompt just typed still appears in the transcript.",
    "turn-no-deltas": "No deltas, only the persisted answer: the answer appears in one piece.",
    "turn-no-reasoning-deltas": "No reasoning deltas: the reasoning surface stays empty and the answer still streams.",
    "turn-empty-delta-text": "Deltas carrying empty text: no empty rows accumulate, and the persisted answer still lands.",
    "turn-ansi-delta": "ANSI escapes inside a delta: sanitised before painting, so a stream cannot clear the screen.",
    "turn-control-delta": "Control characters inside a delta: neutralised while streaming.",
    "turn-rtl-delta": "RTL override text inside a delta: does not corrupt the surrounding layout as it streams.",
    "turn-zwj-delta": "ZWJ emoji arriving across streaming deltas: assembled into whole grapheme clusters.",
    "turn-wide-delta": "Full-width text streaming in: wrapped on cell width rather than byte count.",
    "turn-huge-delta": "A single 60 KB delta: appended without freezing the UI.",
    "turn-newline-delta": "A delta carrying 400 newlines: scrollback grows correctly and stays navigable.",
    "turn-2000-deltas": "2000 deltas in one turn: rendering keeps up and the final answer is complete.",
    "turn-duplicate-seq": "Every envelope reusing seq 1: the repeats are not read as replay loss, and the content is not dropped.",
    "turn-seq-gaps": "seq multiplied sevenfold, leaving gaps: loss is reported once, not once per envelope.",
    "turn-seq-from-zero": "seq shifted down to start at 0: accepted as a valid start rather than read as a missing first envelope.",
    "turn-seq-negative": "Negative envelope seq: does not corrupt gap detection or the cursor.",
    "turn-seq-reversed": "Envelopes delivered in reverse: ordered by seq, so the answer is not printed backwards.",
    "turn-cursor-seq-frozen": "cursor.seq frozen at 1 across the stream: resume state stays consistent and no false loss is reported.",
    "turn-no-cursor": "Envelopes with no cursor: streaming still renders, and only resume degrades.",
    "turn-no-thread-id": "Envelopes with no thread_id: routed to the active thread rather than dropped.",
    "turn-no-turn-id": "Envelopes with no turn_id: still attributed to the running turn.",
    "turn-no-topic": "Envelopes with no topic: not filtered out as belonging to some other topic.",
    "turn-other-topic": "Envelopes stamped topic 'review': ignored by the coding view rather than mixed into it.",
    "turn-full-session-in-envelope": "session_id carrying the full profile:channel:chat#topic form: matched to the open session rather than rejected as foreign.",
    "turn-unknown-payload-type": "Payload type 'hologram_delta': ignored, with the rest of the turn still rendering.",
    "turn-payload-no-data": "A delta payload with no data: skipped rather than unwrapped into a panic.",
    "turn-no-segment-id": "Deltas with no assistant_segment_id: still concatenated into one answer.",
    "turn-mismatched-segment-id": "assistant_persisted naming a different segment: the streamed text is neither duplicated nor orphaned.",
    "turn-persisted-empty-text": "assistant_persisted with empty text: the streamed answer is not wiped by the empty final.",
    "turn-persisted-huge-text": "A 150 KB persisted answer: stored and scrollable without stalling the UI.",
    "turn-persisted-ansi": "ANSI escapes in the persisted answer: sanitised on the final render too, not only while streaming.",
    "turn-persisted-no-meta": "assistant_persisted with no meta: rendered without model or usage annotations.",
    "turn-reasoning-after-answer": "A reasoning delta arriving after the answer: placed in the reasoning surface, not appended to the answer.",
    "turn-progress-unknown-kind": "Progress metadata kind 'teleporting': ignored rather than decoded into an error.",
    "turn-progress-no-metadata": "progress/updated with no metadata: ignored safely.",
    "turn-token-cost-nulls": "Every token_cost field null: the cost line is omitted rather than showing nulls or zeroes.",
    "turn-orchestration-never-idle": "No idle orchestration notification: the client still returns to idle on the terminal envelope.",
    "turn-normalization-no-state": "context/normalization_reported with no context_state: the context meter keeps its previous value.",
    "turn-only-notifications-no-envelopes": "Only non-envelope notifications arrive: the turn produces no answer and still ends cleanly.",
    "turn-user-message-empty": "user_message with empty text: the transcript shows the prompt actually typed, not an empty row.",
    # -------------------------------------------------------------- loops ---
    "loops-empty": "No loops: the loops surface is empty rather than missing or errored.",
    "loops-100": "100 loops: the list renders scrollably without stalling startup.",
    "loops-status-running": "Loops in 'running': shown as active, with their next run.",
    "loops-status-expired": "Loops 'expired': shown as finished rather than as live schedules.",
    "loops-status-unknown": "Loop status 'schroedinger': displayed as unknown rather than dropped.",
    "loops-interval-set": "Interval mode at 300 seconds: shown as a repeating schedule.",
    "loops-interval-zero": "interval_seconds 0: clamped or flagged, not rendered as an infinitely fast loop.",
    "loops-interval-negative": "A negative interval: rejected or clamped, never turned into a negative countdown.",
    "loops-empty-prompt": "An empty loop prompt: the row still identifies the loop.",
    "loops-huge-prompt": "A 40 KB loop prompt: truncated in the list rather than blowing out the layout.",
    "loops-ansi-prompt": "ANSI escapes in a loop prompt: sanitised in the list.",
    "loops-newline-prompt": "A 400-line loop prompt: rendered as one row, not 400.",
    "loops-zero-timestamps": "All loop timestamps zero: shown as unknown rather than as 1970.",
    "loops-negative-timestamps": "Negative loop timestamps: shown as unknown rather than as a date before the epoch.",
    "loops-huge-timestamps": "Loop timestamps at 2^53-1: formatted without overflow.",
    "loops-no-loop-id": "Loops with no loop_id: listed but not actionable, rather than crashing the surface.",
    "loops-last-run-set": "last_run_at_ms populated: the last run is shown alongside the next.",
    "loops-session-mismatch": "Loops belonging to another session: filtered out or clearly marked as foreign.",
    # ---------------------------------------------------------------- llm ---
    "llm-primary-null": "No primary model: the client reports that no model is selected rather than presenting an empty selector as ready.",
    "llm-no-fallbacks": "No fallbacks: only the primary is offered.",
    "llm-many-fallbacks": "60 fallbacks: the selector scrolls rather than overflowing.",
    "llm-no-api-key": "No API key on any model: reported as unavailable before a turn is attempted.",
    "llm-route-null": "A null route on the primary: routing shows as unknown rather than blank.",
    "llm-base-url-set": "A custom base_url: surfaced, so it is visible that the model is not going to the default endpoint.",
    "llm-empty-model": "Empty model and model_id: shown as unnamed rather than as a valid selection.",
    "llm-huge-model-name": "A 5 000-character model name: truncated in the chrome.",
    "llm-unknown-provider": "Provider 'acme-ai': displayed as-is rather than dropped for being unrecognised.",
    "llm-nothing-selected": "Nothing marked selected: the client reports no active model instead of silently using the first.",
    "llm-no-runtime-stamp": "No runtime_policy_stamp: policy chrome is hidden rather than filled with defaults presented as facts.",
    "llm-stamp-network-unknown": "network 'maybe': treated as an unknown posture rather than as enabled.",
    "llm-stamp-no-runtime-mode": "No runtime_mode: shown as unknown.",
    "llm-profile-id-mismatch": "The LLM list answers for another profile: rejected or flagged, not shown as this profile's models.",
    # ------------------------------------------------------------- status ---
    "status-health-degraded": "Health 'degraded': surfaced in the status line while the session stays usable.",
    "status-health-error": "Health 'error' with a detail: the detail is shown, and the transcript is still readable.",
    "status-health-unknown": "Health '???': shown as unknown rather than as healthy.",
    "status-no-health": "No health block: the indicator is hidden rather than assumed green.",
    "status-usage-populated": "Usage figures present: tokens and cost appear in the status line.",
    "status-cursor-unhealthy": "cursor.healthy false: the client warns that streaming may be lossy.",
    "status-no-replay": "replay_supported false: replay is disabled rather than attempted and failing.",
    "status-no-model": "No model block in status: the model indicator falls back to the profile list or shows unknown.",
    "status-model-unselected": "The status model marked unselected: consistent with the selector rather than shown as active.",
    "status-mcp-nonzero": "An MCP summary with failures and servers connecting: the counts are surfaced, failures visibly.",
    "status-tools-nonzero": "42 visible tools with 2 denied: the counts are surfaced accurately.",
    "status-permission-unknown": "permission_profile 'omniscient': shown as an unknown profile rather than silently treated as permissive.",
    "status-sandbox-unknown": "sandbox 'none-at-all': shown verbatim; an unknown value must not be read as sandboxed.",
    "status-no-capabilities": "status with no capabilities block: the client relies on config/capabilities/list rather than erroring.",
    "status-limits-zero": "Zero agent, loop and depth limits: the features report as unavailable rather than being offered and failing later.",
    "status-limits-negative": "Negative limits: rejected or clamped, never turned into a negative allowance.",
    "status-budget-zero": "Zero goal token budgets: goals report as unavailable rather than instantly exhausted.",
    "status-workspace-null": "workspace_root null: the cwd indicator shows unknown rather than '/' or empty.",
    "status-no-context-state": "status with no context_state: the context meter falls back to hydrate's copy or hides.",
    "status-session-mismatch": "status answers for a different session: flagged rather than applied to the open session.",
    # ------------------------------------------------------------- agents ---
    "agents-one-running": "One running agent: listed with its role and objective.",
    "agents-many": "40 agents: the list scrolls rather than truncating startup.",
    "agents-unknown-state": "Agent state 'vibing': shown as unknown rather than dropped.",
    "agents-no-agent-id": "An agent with no agent_id: listed but not actionable, rather than crashing the surface.",
    "agents-huge-objective": "A 50 KB objective: truncated in the list.",
    "agents-deep-tree": "Agents nested 11 deep: rendered, or depth-capped, without runaway indentation.",
    # --------------------------------------------------------------- goal ---
    "goal-set": "An active goal with a 2M budget: objective and remaining budget are shown.",
    "goal-budget-exhausted": "A goal with its budget spent: shown as exhausted, with the client explaining why further work is blocked.",
    "goal-huge-objective": "A 60 KB objective: truncated in the goal chrome.",
    "goal-null-fields": "Every goal field null: shown as no goal rather than as an empty goal card.",
    "goal-unknown-status": "Goal status 'quantum': shown as unknown rather than as active.",
    # ------------------------------------------- cross-payload additions ---
    "hydrate-recovery-rebuilt": "Context recovery_state 'rebuilt' rather than 'exact', on hydrate, status/read and the turn stream alike: hydrate decodes and the full transcript renders, but the session is reporting that its context was REBUILT rather than recovered intact — the client should say replayed history may not be faithful. It draws nothing today (summary_line() is test-only), so no visible change is the current behaviour, not the wanted one.",
}


# ------------------------------------------------------------------ emit ---
# Which JSON-RPC method each file answers, for the heartbeat's self-description.
METHOD_OF = {
    "cap": "config/capabilities/list",
    "llm": "profile/llm/list",
    "status": "session/status/read",
    "hydrate": "session/hydrate",
    "agents": "agent/list",
    "goal": "session/goal/get",
    "loops": "loop/list",
    "turn": "turn/start (pushed notifications)",
}


def _kind(v):
    return {dict: "object", list: "array", str: "string", bool: "bool",
            int: "number", float: "number", type(None): "null"}.get(type(v), "?")


def _short(v):
    text = json.dumps(v, ensure_ascii=False)
    return text if len(text) <= 60 else text[:57] + "..."


def diff_paths(a, b, prefix="", out=None, cap=16):
    """Dotted paths of everything that differs between pristine and mutated."""
    if out is None:
        out = []
    if len(out) >= cap:
        return out
    if type(a) is not type(b) and not (isinstance(a, bool) or isinstance(b, bool)):
        out.append(f"{prefix or '(root)'}: {_kind(a)} -> {_kind(b)}")
    elif isinstance(a, dict):
        for key in sorted(set(a) | set(b)):
            path = f"{prefix}.{key}" if prefix else key
            if key not in b:
                out.append(f"removed {path}")
            elif key not in a:
                out.append(f"added {path} = {_short(b[key])}")
            else:
                diff_paths(a[key], b[key], path, out, cap)
    elif isinstance(a, list):
        if len(a) != len(b):
            out.append(f"{prefix}[] length {len(a)} -> {len(b)}")
            # A wholesale list swap otherwise hides the field that actually
            # changed: "messages[] 30 -> 1" says nothing about the empty content
            # that is the point of the scenario.
            if a and b:
                diff_paths(a[0], b[0], f"{prefix}[0]", out, cap)
        else:
            for i, (x, y) in enumerate(zip(a, b)):
                diff_paths(x, y, f"{prefix}[{i}]", out, cap)
    elif a != b:
        out.append(f"{prefix} = {_short(b)}")
    return out


def materialise(index, name, area, build):
    """One scenario's overriding files, keyed by FILES key, heartbeat included.

    Shared with setup_scenario.py rather than reimplemented there: `index` is the
    scenario_id a whole campaign's results are keyed on, and two copies of this
    would eventually disagree about what case 137 is.
    """
    files = build()
    # Stamp every scenario with its own identity, so the deployed mock can
    # be asked which scenario it is instead of being assumed.
    modified = []
    for key, value in files.items():
        paths = diff_paths(BASE[key], value)
        modified.append({"method": METHOD_OF[key], "file": FILES[key],
                         "fields": paths[:16],
                         "truncated": len(paths) >= 16})
    files["heartbeat"] = {"scenario": name, "scenario_id": index, "area": area,
                          "expectation": EXPECTATIONS[name],
                          "modified": modified,
                          "session_id": "alan:local:tui#coding", "profile_id": "alan"}
    return files


def check_expectations():
    """Every scenario needs one, and none may name a case that does not exist.

    A scenario with no expectation deploys and runs looking exactly like every
    other one; the gap shows up as a blank step in a report nobody reads until
    the campaign is over.
    """
    missing = [name for name, _, _ in SCENARIOS if name not in EXPECTATIONS]
    if missing:
        raise SystemExit("no expectation for: " + ", ".join(missing))
    unused = sorted(set(EXPECTATIONS) - {name for name, _, _ in SCENARIOS})
    if unused:
        raise SystemExit("expectation for a scenario that does not exist: " + ", ".join(unused))


def main():
    check_expectations()
    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)
    manifest = []
    for index, (name, area, build) in enumerate(SCENARIOS, start=1):
        files = materialise(index, name, area, build)
        folder = os.path.join(OUT, f"{index:03}")
        os.makedirs(folder)
        for key, value in files.items():
            with open(os.path.join(folder, FILES[key]), "w", encoding="utf-8") as handle:
                json.dump(value, handle, ensure_ascii=False)
        manifest.append({"id": index, "name": name, "area": area, "files": sorted(FILES[k] for k in files)})
    with open(os.path.join(OUT, "manifest.json"), "w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=1)
    areas = {}
    for entry in manifest:
        areas[entry["area"]] = areas.get(entry["area"], 0) + 1
    print("scenarios:", len(manifest))
    for area, count in sorted(areas.items()):
        print(f"  {area:14} {count}")


if __name__ == "__main__":
    main()
