//! Capture the prompt Claude Code sends, and nothing else.
//!
//! Sits on the martian MITM proxy (`:20810`) and hooks the Anthropic messages
//! endpoint. For every request it records ONE thing on the report: the text of
//! the LAST user message. `scripts/ab_replay.py` reads those back and replays
//! them at octoscode so the two agents can be compared on identical input.
//!
//! ## What is deliberately NOT captured
//!
//! `/v1/messages` carries the whole conversation, not a prompt: the system
//! prompt, every tool definition, and every prior message — which for a coding
//! agent means the contents of every file it has read, and any secret that
//! appeared in any of them. Replaying that at another provider would ship all of
//! it off the machine.
//!
//! So this walks to the last `role: "user"` entry, takes its text, and drops the
//! rest on the floor. The recorded step is the only thing that leaves the guest,
//! and it holds one message.
//!
//! ## It must not modify the request
//!
//! `modify http_req` can rewrite what goes upstream. This hook rewrites nothing:
//! the tap is on the user's real Claude Code session, and a tap that alters the
//! traffic it observes is no longer measuring the thing it claims to measure.
//! Capture only, byte-for-byte passthrough.
extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_macro;
extern crate wasm_mock_util;
use wasm_mock_macro::mock_suite;
use wasm_mock_util::RequestReceivedInMock;
use wasm_mock_util::*;

/// Longest prompt recorded. A step is a report row, not a blob store, and a
/// pasted stack trace should not push the rest of the run out of the report.
const MAX_PROMPT: usize = 8000;

/// The path this suite hooks, and the report key to file steps against.
///
/// It has to be a literal here rather than read from the guest's `COMMAND`
/// static, because on this route the host never populates it: `common_mock.go`
/// invokes `save_command`/`save_uid` around the TCP path, but the HTTP fiddler
/// reaches `_modify_req` with both still empty. A probe confirmed it —
/// `host_call` returned Ok while `COMMAND` read as "", so every step was filed
/// against report "" and dropped without an error anyone could see.
///
/// `CallFiddler` stores `mock_targets` -> report uid, and `cli/test_suite.xml`
/// sets `mock` to this same path, so `AddStep`'s `MockCommandUidMap` fallback
/// resolves it. Keep the three in sync: this literal, the `modify http_req`
/// pattern below, and `<mock>` in the suite config.
const ROUTE: &str = "/v1/messages";

/// Prompt seen in the request hook, waiting for a report row to exist.
///
/// Single-slot rather than a queue: the host drives request→response in pairs on
/// one instance, so at most one is ever in flight, and holding more would mean
/// holding conversation text longer than the moment it takes to write it out.
static mut PENDING_PROMPT: Option<String> = None;

/// Text of the last `user` message in an Anthropic `/v1/messages` body.
///
/// `content` is either a plain string or an array of typed blocks; only `text`
/// blocks are joined. Non-text blocks (`tool_result`, `image`) are skipped —
/// they are context, not the thing a person typed, and a tool_result is exactly
/// the kind of payload this must not forward.
/// Internal machinery that arrives on a `user` turn but is not a person asking
/// anything. Claude Code makes side-calls for autocomplete and injects context
/// blocks into the user role, and replaying either at octoscode compares the
/// two agents on plumbing rather than on a task.
fn is_machinery(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("[SUGGESTION MODE:")
        || t.starts_with("<system-reminder>")
        || t.starts_with("<command-")
        || t.starts_with("<local-command-")
        || t.starts_with("Analyze this conversation")
}

/// Drop `<system-reminder>…</system-reminder>` spans.
///
/// These ride along inside the same user message as the real prompt and carry
/// harness context — the user's email address, for one — so they must not be
/// recorded even when a genuine question sits beside them.
fn strip_reminders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<system-reminder>") {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find("</system-reminder>") {
            Some(end) => &rest[start + end + "</system-reminder>".len()..],
            // Unterminated: drop the remainder rather than record half a block.
            None => "",
        };
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn last_user_prompt(body: &serde_json::Value) -> Option<String> {
    let messages = body.get("messages")?.as_array()?;
    for message in messages.iter().rev() {
        if message.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let content = message.get("content")?;
        let text = match content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(blocks) => blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };
        let text = strip_reminders(&text);
        if is_machinery(&text) {
            // A side-call, not a turn. Nothing in this request is a prompt, so
            // stop rather than walking further back into the transcript.
            return None;
        }
        if text.is_empty() {
            // A user turn carrying only tool_results is the agent loop talking
            // to itself, not a person asking something. Keep looking back.
            continue;
        }
        let mut text = text;
        if text.len() > MAX_PROMPT {
            text.truncate(MAX_PROMPT);
            text.push_str(" …[truncated]");
        }
        return Some(text);
    }
    None
}

/// File a step on the report, binding on COMMAND rather than UID.
///
/// `foo_step!` binds on the `UID` static, and `UID` is empty while this hook
/// runs. `common_mock.go` invokes the guest in this order:
///
///     instance.Invoke(ctx, "save_command", element_new)   // COMMAND set
///     instance.Invoke(ctx, element_new + "_..._modify_req")  // <- we run here
///     instance.Invoke(ctx, "save_uid", uID)               // UID set, too late
///
/// so a `foo_step!` from here calls `AddStep` with an empty binding, the
/// `MockCommandUidMap` lookup misses, and the step is filed against report ""
/// and silently dropped. That is why the first two captures came back with a
/// row but zero `AB-PROMPT` steps.
///
/// `COMMAND` holds `element_new` — the path this suite is registered under, and
/// exactly the key `CallFiddler` stored in `MockCommandUidMap`. Binding on it
/// makes `AddStep`'s fallback resolve the live report.
fn step_on_command(desc: String) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    host_call(ROUTE, "foo", "step_fail", desc.as_bytes())?;
    Ok(())
}

#[no_mangle]
pub extern "C" fn _start() {
    mock_suite! {
        name ab_prompt_tap;

        modify http_req "/v1/messages" (req) {
            // Recorded as a FAILING step on purpose: `appendStep` keeps failures
            // where a pass is easy to scroll past, and a captured prompt is a
            // to-do (replay it) rather than an assertion that held. The
            // `AB-PROMPT:` prefix is what ab_replay.py greps for.
            // Stash only. The step CANNOT be filed from here: `AppendStep`
            // drops a step while the report has no test rows, and the row for
            // this request is not appended until after this hook returns. The
            // host says so out loud —
            //   appendStep err <uid> AB-PROMPT: PROBE SEVEN
            // — which is the whole reason five earlier captures came back empty
            // with `host_call` reporting success. The response hook below runs
            // once the row exists.
            unsafe {
                #[allow(static_mut_refs)]
                {
                    PENDING_PROMPT = last_user_prompt(&req.HttpBody);
                }
            }
            // No mutation. See the module docs.
        }

        modify http_res "/v1/messages" (res) {
            // The request row exists by now, so a step will stick.
            let pending = unsafe {
                #[allow(static_mut_refs)]
                PENDING_PROMPT.take()
            };
            match pending {
                Some(prompt) => {
                    let _ = step_on_command(format!("AB-PROMPT: {prompt}"));
                }
                None => {
                    let _ = step_on_command(String::from(
                        "AB-PROMPT-MISS: no user text in body",
                    ));
                }
            }
            // Responses are never modified either — see the module docs.
            let _ = res;
        }
    }
}

fn main() {}
