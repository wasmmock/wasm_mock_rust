//! A standalone octos UI Protocol server, replayed from a `tcp_fiddler` capture
//! of a TUI session. Nothing is proxied: the guest answers the client itself by
//! writing frames back down the local connection with the mock server's
//! `tcp_response` capability, so no octos server needs to be running.
//!
//! The capture pairs a request with whatever the server happened to send while
//! that request was in flight, so `MockData[i].request` does NOT identify the
//! reply next to it — index 2's `profile/llm/list` sits beside the answer to
//! `tui-1`'s `config/capabilities/list`. What pairs a reply with its question is
//! the JSON-RPC `id`, so the bodies below are keyed by method and the live id is
//! echoed back onto them.
//!
//! ## The remote port has not gone away, it has been defanged
//!
//! `PoolInit` resolves and dials the remote half of the port map, and the
//! goroutine that reads the client waits for that dial to land before its first
//! `Read` (`tcpproxy/pool.go`: "upstream never came up"). Point the map at a
//! dead port and the guest is never invoked at all. So the map keeps a remote
//! that is certain to accept — the mock server's own HTTP port — and simply
//! never writes to it: every payload returns "/continue", which is what tells
//! the host to forward nothing.
//!
//! ## What is answered
//!
//! The seven bodies in `canned` are replayed verbatim: they are the replies the
//! capture recorded, which is exactly the set small enough to fit one WebSocket
//! frame and so reach the guest decoded rather than forwarded as split bytes.
//!
//! Three more are served without a recorded body, because the capture could not
//! have one:
//!
//! - `session/open` — the real reply is ~55 KB and was forwarded unrecorded.
//!   `session_open_result` assembles `{"opened": …}` from `session/status/read`
//!   and `session/hydrate`, so every field still comes from captured data.
//! - `turn/start` — answered with `{"accepted": true}`, because a turn is
//!   delivered as pushed notifications rather than a result. `turn_stream`
//!   replays those, retargeted at the live turn_id and prompt.
//! - `launch/resolve` — answered `resume`, which is the only decision that
//!   explains what the client did next in the capture (open a session rather
//!   than prompt).
//! - `mcp/status/list` and `tool/status/list` — reported empty, which is what
//!   the capture's own `mcp_summary`, `tool_summary` and `mcp_servers` say the
//!   session had.
//!
//! Result shapes come from the protocol crate (`SessionOpenResult`,
//! `TurnStartResult`, `LaunchResolveResult`) and, for the two status lists,
//! from the client's own decode structs plus the server handlers that build
//! them — not guessed.
//!
//! Every method the capture never saw gets an explicit JSON-RPC error. With no
//! upstream to fall through to, silence is indistinguishable from a hang.
extern crate wapc_guest as guest;
use guest::prelude::*;
extern crate wasm_mock_util;
extern crate httparse;
extern crate serde_json;
use base64::{engine::general_purpose, Engine as _};
use bytes::BytesMut;
use lazy_static::lazy_static;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::codec::{Decoder, Encoder};
use wasm_mock_util::*;
use websocket_codec::{MessageCodec, Opcode};

/// Local port the TUI connects to, and a remote that exists only to satisfy the
/// proxy's dial. Both halves must match the registered function names, so they
/// come from one macro rather than three string literals.
macro_rules! port_map {
    () => {
        "3335-:20825"
    };
}
/// Also the `tcp_response` binding: the host selects the client connection with
/// `strings.Contains(key, portmap)`.
const PORT_MAP: &str = port_map!();

/// The session the capture was taken against. Canned bodies embed it in dozens
/// of places; `retarget` swaps it for whatever session the live client opens so
/// the mock is not tied to one profile's session name.
const CAPTURED_SESSION: &str = "alan:local:tui#coding";

/// Profile the capture ran under. Used only as the `launch/resolve` fallback,
/// for a client that sends no `profile_id` — the session id it then derives is
/// the one the canned bodies already carry.
const CAPTURED_PROFILE: &str = "alan";

/// Per-connection framing state. The host hands us raw TCP payloads, so the
/// upgrade and the frame boundaries are ours to track.
struct Conn {
    upgraded: bool,
    buf: BytesMut,
    /// Decodes the client's masked frames.
    decoder: MessageCodec,
    /// Encodes our unmasked ones. A server must not mask, and reusing the
    /// request codec here would produce frames the client drops.
    encoder: MessageCodec,
}

impl Conn {
    fn new() -> Self {
        Conn {
            upgraded: false,
            buf: BytesMut::new(),
            decoder: MessageCodec::client(),
            encoder: MessageCodec::server(),
        }
    }
}

lazy_static! {
    /// Keyed on "{Laddr}-{Raddr}", the same identity the host reports.
    static ref CHANNELS: Mutex<HashMap<String, Conn>> = Mutex::new(HashMap::new());
    /// session_id the live client is using, taken from request params.
    static ref SESSION: Mutex<String> = Mutex::new(String::new());
}

const CAPABILITIES: &str = r#"{"capabilities":{"version":{"protocol":"octos-ui/v1alpha1","schema_version":1,"jsonrpc":"2.0"},"capabilities_schema_version":2,"supported_methods":["session/open","turn/start","turn/interrupt","approval/respond","approval/scopes/list","session/btw","user_question/respond","permission/profile/list","permission/profile/set","diff/preview/get","task/list","task/cancel","task/restart_from_node","task/output/read","session/hydrate","session/rollback","session/fork","agent/list","agent/status/read","agent/output/read","agent/artifact/list","agent/artifact/read","agent/interrupt","agent/close","session/goal/get","session/goal/set","session/goal/clear","loop/create","loop/list","loop/delete","loop/pause","loop/resume","loop/fire_now","router/set_mode","router/get_metrics","launch/resolve","client_hello","config/capabilities/list","session/status/read","profile/llm/list","profile/llm/select","mcp/status/list","tool/status/list","auth/status","auth/send_code","auth/verify","auth/me","auth/logout","profile/llm/catalog","profile/llm/upsert","profile/llm/delete","profile/llm/test","profile/llm/fetch_models","profile/sub_providers/list","profile/sub_providers/upsert","profile/sub_providers/remove","snapshot/list","snapshot/restore","peer/prepare","peer/gather","turn/steer","profile/skills/list","profile/skills/registry/search","profile/skills/install","profile/skills/remove","session/compact","session/compact/mode/set"],"supported_notifications":["session/open","turn/started","turn/completed","turn/error","message/delta","message/reasoning_delta","tool/started","tool/progress","tool/completed","approval/requested","approval/auto_resolved","approval/decided","approval/cancelled","user_question/requested","task/updated","plan/updated","task/output/delta","progress/updated","warning","protocol/replay_lossy","turn/spawn_complete","file/attached","visual/generating","visual/succeeded","visual/failed","voice/exit","voice/audio_chunk","projection/envelope","session/event","router/status","router/failover","queue/state","agent/updated","agent/output/delta","agent/artifact/updated","session/goal/updated","session/goal/cleared","loop/updated","loop/fired","loop/completed","context/compaction_completed","context/compaction_started","context/normalization_reported","peer/staged","peer/closed"],"supported_features":["approval.typed.v1","pane.snapshots.v1","session.workspace_cwd.v1","harness.task_control.v1","state.session_hydrate.v1","projection.envelope.v2","coding.autonomy.v1","coding.agent_control.v1","coding.goal_runtime.v1","coding.loop_runtime.v1","context.lifecycle.v1","user_question.v1","plan.todos.v1","permission.profile.v1","runtime.policy_stamp.v1","coding.tool_contract.v1","coding.image_view.v1","coding.dynamic_tool_search.v1","coding.bash.v1","coding.delegate.v1","coding.browser.v1","harness.task_supervision_inspection.v1","harness.task_artifacts.v1"]}}"#;

const PROFILE_LLM_LIST: &str = r#"{"profile_id":"alan","primary":{"provider":"deepseek","model":"deepseek-v4-pro","family_id":"deepseek","model_id":"deepseek-v4-pro","route":{"route_id":"deepseek","label":"Official API","api_key_env":"DEEPSEEK_API_KEY","api_type":"openai"},"route_id":"deepseek","base_url":null,"api_key_env":"DEEPSEEK_API_KEY","has_api_key":true,"selected":true,"available":true},"fallbacks":[{"provider":"deepseek","model":"deepseek-v4-pro","family_id":"deepseek","model_id":"deepseek-v4-pro","route":{"route_id":"deepseek","label":"Official API","api_key_env":"DEEPSEEK_API_KEY","api_type":"openai"},"route_id":"deepseek","base_url":null,"api_key_env":"DEEPSEEK_API_KEY","has_api_key":true,"selected":false,"available":true}],"llm":{"primary":{"provider":"deepseek","model":"deepseek-v4-pro","family_id":"deepseek","model_id":"deepseek-v4-pro","route":{"route_id":"deepseek","label":"Official API","api_key_env":"DEEPSEEK_API_KEY","api_type":"openai"},"route_id":"deepseek","base_url":null,"api_key_env":"DEEPSEEK_API_KEY","has_api_key":true,"selected":true,"available":true},"fallbacks":[{"provider":"deepseek","model":"deepseek-v4-pro","family_id":"deepseek","model_id":"deepseek-v4-pro","route":{"route_id":"deepseek","label":"Official API","api_key_env":"DEEPSEEK_API_KEY","api_type":"openai"},"route_id":"deepseek","base_url":null,"api_key_env":"DEEPSEEK_API_KEY","has_api_key":true,"selected":false,"available":true}]},"runtime_policy_stamp":{"runtime_mode":"solo","profile_id":"alan","workspace_root":null,"approval_policy":"on-request","sandbox_mode":"workspace-write","permission_profile":"workspace_write","filesystem_scope":"workspace","network":"blocked","model":"deepseek-v4-pro","provider":"deepseek","tool_policy_id":"profile","mcp_servers":[],"memory_scope":"profile-session","qoe_policy":"profile","queue_mode":"adaptive","tool_contract_id":"codex-compatible-coding-v1","tool_contract_version":"1","model_toolset":"coding","dynamic_tool_discovery":"enabled"}}"#;

const SESSION_STATUS_READ: &str = r#"{"session_id":"alan:local:tui#coding","profile_id":"alan","runtime_policy_stamp":{"runtime_mode":"solo","profile_id":"alan","workspace_root":"/Users/alanpoon/Documents/rust/robius/octos-tui","approval_policy":"on-request","sandbox_mode":"workspace-write","permission_profile":"workspace_write","filesystem_scope":"workspace","network":"allowed","model":"deepseek-v4-pro","provider":"deepseek","tool_policy_id":"profile","mcp_servers":[],"memory_scope":"profile-session","qoe_policy":"profile","queue_mode":"adaptive","tool_contract_id":"codex-compatible-coding-v1","tool_contract_version":"1","model_toolset":"coding","dynamic_tool_discovery":"enabled","autonomy_contract_id":"coding-autonomy-v1","agent_control":"available","goal_runtime":"available","loop_runtime":"available","goal_default_token_budget":2000000,"goal_max_token_budget":1000000000,"continuation_min_delay_seconds":30,"continuation_max_per_hour":20,"loop_min_interval_seconds":60,"loop_max_interval_seconds":86400,"loop_max_age_days":7,"loop_allow_slash_commands":true,"idle_only_scheduling":true,"max_objective_bytes":8192,"max_loop_prompt_bytes":8192,"max_loops_per_session":16,"max_agent_tree_depth":4,"max_agents_per_session":32},"context":{"schema":"octos.context.lifecycle.v1","state":{"session_id":"alan:local:tui#coding","thread_id":null,"generation":68,"transcript_hash":"sha256:8e03e5c4dc7727c4626feccc2fd06b1d07b38a755be4ff5d90c6b28614d9d9bb","last_checkpoint_id":null,"last_compaction_id":null,"token_estimate":1937,"item_count":47,"recovery_state":"exact"},"compaction":{"count":0,"last":null}},"context_state":{"session_id":"alan:local:tui#coding","generation":68,"transcript_hash":"sha256:8e03e5c4dc7727c4626feccc2fd06b1d07b38a755be4ff5d90c6b28614d9d9bb","item_count":47,"token_estimate":1937,"recovery_state":"exact"},"permission_profile":"workspace_write","sandbox":"workspace-write","health":{"status":"ok"},"mcp_summary":{"connected":0,"connecting":0,"failed":0,"disabled":0},"tool_summary":{"visible":0,"enabled":0,"denied":0,"policy_id":"profile"},"usage":{},"cursor":{"healthy":true,"replay_supported":true},"capabilities":{"version":{"protocol":"octos-ui/v1alpha1","schema_version":1,"jsonrpc":"2.0"},"capabilities_schema_version":2,"supported_methods":["session/open","turn/start","turn/interrupt","approval/respond","approval/scopes/list","session/btw","user_question/respond","permission/profile/list","permission/profile/set","diff/preview/get","task/list","task/cancel","task/restart_from_node","task/output/read","session/hydrate","session/rollback","session/fork","agent/list","agent/status/read","agent/output/read","agent/artifact/list","agent/artifact/read","agent/interrupt","agent/close","session/goal/get","session/goal/set","session/goal/clear","loop/create","loop/list","loop/delete","loop/pause","loop/resume","loop/fire_now","router/set_mode","router/get_metrics","launch/resolve","client_hello","config/capabilities/list","session/status/read","profile/llm/list","profile/llm/select","mcp/status/list","tool/status/list","auth/status","auth/send_code","auth/verify","auth/me","auth/logout","profile/llm/catalog","profile/llm/upsert","profile/llm/delete","profile/llm/test","profile/llm/fetch_models","profile/sub_providers/list","profile/sub_providers/upsert","profile/sub_providers/remove","snapshot/list","snapshot/restore","peer/prepare","peer/gather","turn/steer","profile/skills/list","profile/skills/registry/search","profile/skills/install","profile/skills/remove","session/compact","session/compact/mode/set"],"supported_notifications":["session/open","turn/started","turn/completed","turn/error","message/delta","message/reasoning_delta","tool/started","tool/progress","tool/completed","approval/requested","approval/auto_resolved","approval/decided","approval/cancelled","user_question/requested","task/updated","plan/updated","task/output/delta","progress/updated","warning","protocol/replay_lossy","turn/spawn_complete","file/attached","visual/generating","visual/succeeded","visual/failed","voice/exit","voice/audio_chunk","projection/envelope","session/event","router/status","router/failover","queue/state","agent/updated","agent/output/delta","agent/artifact/updated","session/goal/updated","session/goal/cleared","loop/updated","loop/fired","loop/completed","context/compaction_completed","context/compaction_started","context/normalization_reported","peer/staged","peer/closed"],"supported_features":["approval.typed.v1","pane.snapshots.v1","session.workspace_cwd.v1","harness.task_control.v1","state.session_hydrate.v1","projection.envelope.v2","coding.autonomy.v1","coding.agent_control.v1","coding.goal_runtime.v1","coding.loop_runtime.v1","context.lifecycle.v1","user_question.v1","plan.todos.v1","permission.profile.v1","runtime.policy_stamp.v1","coding.tool_contract.v1","coding.image_view.v1","coding.dynamic_tool_search.v1","coding.bash.v1","coding.delegate.v1","coding.browser.v1","harness.task_supervision_inspection.v1","harness.task_artifacts.v1"]},"model":{"model":"deepseek-v4-pro","provider":"deepseek","selected":true}}"#;

const SESSION_HYDRATE: &str = r#"{"session_id":"alan:local:tui#coding","cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1613},"context":{"schema":"octos.context.lifecycle.v1","state":{"session_id":"alan:local:tui#coding","thread_id":null,"generation":68,"transcript_hash":"sha256:8e03e5c4dc7727c4626feccc2fd06b1d07b38a755be4ff5d90c6b28614d9d9bb","last_checkpoint_id":null,"last_compaction_id":null,"token_estimate":1937,"item_count":47,"recovery_state":"exact"},"compaction":{"count":0,"last":null}},"context_state":{"session_id":"alan:local:tui#coding","generation":68,"transcript_hash":"sha256:8e03e5c4dc7727c4626feccc2fd06b1d07b38a755be4ff5d90c6b28614d9d9bb","item_count":47,"token_estimate":1937,"recovery_state":"exact"},"messages":[{"seq":0,"role":"user","content":"hello","thread_id":"019fea57-d50c-7d91-88ab-12649bfcff8f","persisted_at":"2026-08-10T06:24:12.686828Z","message_id":"alan:local:tui#coding:0:1786343052686828000"},{"seq":1,"role":"assistant","content":"Hello! 👋 How can I help you today?","thread_id":"019fea57-d50c-7d91-88ab-12649bfcff8f","persisted_at":"2026-08-10T06:24:15.835456Z","reasoning_content":"The user is just saying hello. This is a simple greeting, no tools needed.","message_id":"alan:local:tui#coding:1:1786343055835456000"},{"seq":2,"role":"user","content":"hello","thread_id":"019fea58-edcd-79e2-b972-0fb3557b6d4e","persisted_at":"2026-08-10T06:25:24.459647Z","message_id":"alan:local:tui#coding:2:1786343124459647000"},{"seq":3,"role":"assistant","content":"Hello again! 😊 What can I do for you?","thread_id":"019fea58-edcd-79e2-b972-0fb3557b6d4e","persisted_at":"2026-08-10T06:25:27.658146Z","reasoning_content":"The user is just saying \"hello\" again. I should respond simply without any complex processing.","message_id":"alan:local:tui#coding:3:1786343127658146000"},{"seq":4,"role":"user","content":"hello","thread_id":"019fea5a-caa0-7b20-b670-3dbd4f8e0c86","persisted_at":"2026-08-10T06:27:26.551173Z","message_id":"alan:local:tui#coding:4:1786343246551173000"},{"seq":5,"role":"assistant","content":"Hello! 😊 Is there something you'd like to ask or do? I'm here to help with research, coding, weather, news, or whatever you need.","thread_id":"019fea5a-caa0-7b20-b670-3dbd4f8e0c86","persisted_at":"2026-08-10T06:27:29.235648Z","reasoning_content":"The user is saying hello again. This is the third time. I'll respond with a friendly greeting and prompt them to share what they need.","message_id":"alan:local:tui#coding:5:1786343249235648000"},{"seq":6,"role":"user","content":"who is my dad?","thread_id":"019fea73-7c31-7942-872e-ad847fef2290","persisted_at":"2026-08-10T06:54:24.953381Z","message_id":"alan:local:tui#coding:6:1786344864953381000"},{"seq":7,"role":"assistant","content":"I don't have any information about your dad — we haven't talked about your family before, and I don't have access to personal details unless you've shared them with me.\n\nWould you like to tell me about him so I can remember for next time?","thread_id":"019fea73-7c31-7942-872e-ad847fef2290","persisted_at":"2026-08-10T06:54:29.943365Z","reasoning_content":"The user is asking \"who is my dad?\" This is a personal question about their family. I don't have access to any personal information about the user from the conversation so far - we've just exchanged \"hello\" messages. I should let them know I don't have this information and ask if they'd like me to save it for future reference.","message_id":"alan:local:tui#coding:7:1786344869943365000"},{"seq":8,"role":"user","content":"hello","thread_id":"019fea8c-2af2-71d0-a19c-57505af03927","persisted_at":"2026-08-10T07:21:22.545976Z","message_id":"alan:local:tui#coding:8:1786346482545976000"},{"seq":9,"role":"assistant","content":"Hi again! 😊 What can I help you with today?","thread_id":"019fea8c-2af2-71d0-a19c-57505af03927","persisted_at":"2026-08-10T07:21:25.066459Z","reasoning_content":"The user is saying \"hello\" again. This is a simple greeting. I'll respond with a friendly greeting and offer help.","message_id":"alan:local:tui#coding:9:1786346485066459000"},{"seq":10,"role":"user","content":"hello","thread_id":"019feb4f-efd6-79d2-afdd-9246bfff1528","persisted_at":"2026-08-10T10:55:12.418840Z","message_id":"alan:local:tui#coding:10:1786359312418840000"},{"seq":11,"role":"assistant","content":"Hello! 👋 Seems like we're doing a lot of greeting today. Is there something specific you'd like to try, or are you just testing things out? Either way, I'm here to help!","thread_id":"019feb4f-efd6-79d2-afdd-9246bfff1528","persisted_at":"2026-08-10T10:55:16.029857Z","reasoning_content":"The user is saying \"hello\" again. They've said hello many times in this conversation. I'll just respond with a friendly greeting and ask if there's something specific they'd like help with.","message_id":"alan:local:tui#coding:11:1786359316029857000"},{"seq":12,"role":"user","content":"testing","thread_id":"019feb52-4b72-7ae3-9282-ff3f7101c36d","persisted_at":"2026-08-10T10:57:46.931155Z","message_id":"alan:local:tui#coding:12:1786359466931155000"},{"seq":13,"role":"assistant","content":"Got it — everything's working on my end! ✅ If you want to test something specific (search, coding, a tool, whatever), just say the word.","thread_id":"019feb52-4b72-7ae3-9282-ff3f7101c36d","persisted_at":"2026-08-10T10:57:50.502172Z","reasoning_content":"The user says \"testing\" — seems like they're just checking if I respond. I'll give a friendly confirmation that everything is working and offer to help with anything they might need.","message_id":"alan:local:tui#coding:13:1786359470502172000"},{"seq":14,"role":"user","content":"test 1","thread_id":"019feb64-4990-7ef2-bda1-ecfe708b097b","persisted_at":"2026-08-10T11:17:26.179154Z","message_id":"alan:local:tui#coding:14:1786360646179154000"},{"seq":15,"role":"assistant","content":"Test 1 acknowledged! 🧪 What would you like me to test? Some ideas:\n\n- **Search** — look something up online\n- **Weather** — check the weather in a city\n- **Time** — get the current time anywhere\n- **Code** — run a command or read a file\n- **Voice** — text-to-speech or transcription\n\nJust let me know what to try!","thread_id":"019feb64-4990-7ef2-bda1-ecfe708b097b","persisted_at":"2026-08-10T11:17:30.004198Z","reasoning_content":"The user is saying \"test 1\" — this seems like they're continuing to test things. Let me just acknowledge and ask what specific test they'd like to run.","message_id":"alan:local:tui#coding:15:1786360650004198000"},{"seq":16,"role":"user","content":"test 2","thread_id":"019feb66-3adb-7272-9578-a3dc217c1144","persisted_at":"2026-08-10T11:19:33.392120Z","message_id":"alan:local:tui#coding:16:1786360773392120000"},{"seq":17,"role":"assistant","content":"Test 2 acknowledged! ✅ Still here and ready. What's next — or should I actually *do* something on test 3?","thread_id":"019feb66-3adb-7272-9578-a3dc217c1144","persisted_at":"2026-08-10T11:19:36.521052Z","reasoning_content":"The user just said \"test 2\" without further instructions. This seems like they're running through a simple test sequence. I should acknowledge it and ask what they'd like to test next, keeping it brief since we're in a testing flow.","message_id":"alan:local:tui#coding:17:1786360776521052000"},{"seq":18,"role":"user","content":"test 3","thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","persisted_at":"2026-08-10T11:26:34.200563Z","message_id":"alan:local:tui#coding:18:1786361194200563000"},{"seq":19,"role":"assistant","content":"Let me actually do something this time — here's the current time:","thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","persisted_at":"2026-08-10T11:26:37.492307Z","reasoning_content":"The user is saying \"test 3\" — this seems like they're just doing a series of sequential test messages. Let me do something concrete this time to show I can actually perform actions. Let me do a quick time check — it's fast, concrete, and demonstrates tool usage.","message_id":"alan:local:tui#coding:19:1786361197492307000"},{"seq":20,"role":"tool","content":"2026-08-10 19:26:37 (UTC+08:00)\nMonday, Local","thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","persisted_at":"2026-08-10T11:26:37.772125Z","message_id":"alan:local:tui#coding:20:1786361197772125000"},{"seq":21,"role":"assistant","content":"There you go! 🕐 \n\n**Current time:** Monday, August 10, 2026, 7:26 PM (UTC+8)\n\nTest 3 — we did a real tool call. Want me to try something else for test 4?","thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","persisted_at":"2026-08-10T11:26:41.176186Z","reasoning_content":"The time is 2026-08-10 19:26:37 (UTC+08:00), Monday. Let me present this to the user.","message_id":"alan:local:tui#coding:21:1786361201176186000"},{"seq":22,"role":"user","content":"test 4","thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","persisted_at":"2026-08-10T11:27:31.154042Z","message_id":"alan:local:tui#coding:22:1786361251154042000"},{"seq":23,"role":"assistant","content":"Let's try the weather this time:","thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","persisted_at":"2026-08-10T11:27:34.010404Z","reasoning_content":"The user wants \"test 4\". Let me do another tool call - something different this time. Let me try weather or a search.","message_id":"alan:local:tui#coding:23:1786361254010404000"},{"seq":24,"role":"tool","content":"San Francisco, California, United States\nFoggy\nTemperature: 13.6°C (feels like 12.7°C)\nHumidity: 100%\nWind: 13.9 km/h SW","thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","persisted_at":"2026-08-10T11:27:36.928158Z","message_id":"alan:local:tui#coding:24:1786361256928158000"},{"seq":25,"role":"assistant","content":"Test 4 — real weather data! 🌫️\n\n**San Francisco:**\n- ☁️ Foggy\n- 🌡️ 13.6°C (feels like 12.7°C)\n- 💧 Humidity: 100%\n- 💨 Wind: 13.9 km/h SW\n\nClassic SF summer. Anything else for test 5?","thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","persisted_at":"2026-08-10T11:27:39.914882Z","reasoning_content":"The weather tool worked. Let me present the results.","message_id":"alan:local:tui#coding:25:1786361259914882000"},{"seq":26,"role":"user","content":"tes","thread_id":"019feb6e-982b-7ad0-b228-492b6d919545","persisted_at":"2026-08-10T11:28:41.560019Z","message_id":"alan:local:tui#coding:26:1786361321560019000"},{"seq":27,"role":"assistant","content":"Did you mean \"test 5\"? Or did your message get cut off? Happy to continue however you'd like!","thread_id":"019feb6e-982b-7ad0-b228-492b6d919545","persisted_at":"2026-08-10T11:28:44.333529Z","reasoning_content":"The user typed \"tes\" which seems like an interrupted or cut-off message. It could be \"test 5\" or \"testing\" or something else. I'll ask what they meant.","message_id":"alan:local:tui#coding:27:1786361324333529000"},{"seq":28,"role":"user","content":"asd","thread_id":"019febb9-b1a6-7b00-9308-e7e09bf2cb8a","persisted_at":"2026-08-10T12:50:43.291013Z","message_id":"alan:local:tui#coding:28:1786366243291013000"},{"seq":29,"role":"assistant","content":"Looks like some random keys slipped through — `asd`! 😄\n\nWas that intentional, or did you mean to type something? I'm still here if you need anything!","thread_id":"019febb9-b1a6-7b00-9308-e7e09bf2cb8a","persisted_at":"2026-08-10T12:50:46.698263Z","reasoning_content":"The user sent \"asd\" which seems like random keyboard input — probably a test or accidentally hitting keys. I'll keep it light and ask if they meant something specific.","message_id":"alan:local:tui#coding:29:1786366246698263000"}],"threads":[{"thread_id":"019fea57-d50c-7d91-88ab-12649bfcff8f","root_seq":0,"message_seqs":[0,1],"status":"unknown"},{"thread_id":"019fea58-edcd-79e2-b972-0fb3557b6d4e","root_seq":2,"message_seqs":[2,3],"status":"unknown"},{"thread_id":"019fea5a-caa0-7b20-b670-3dbd4f8e0c86","root_seq":4,"message_seqs":[4,5],"status":"unknown"},{"thread_id":"019fea73-7c31-7942-872e-ad847fef2290","root_seq":6,"message_seqs":[6,7],"status":"unknown"},{"thread_id":"019fea8c-2af2-71d0-a19c-57505af03927","root_seq":8,"message_seqs":[8,9],"status":"unknown"},{"thread_id":"019feb4f-efd6-79d2-afdd-9246bfff1528","root_seq":10,"message_seqs":[10,11],"status":"unknown"},{"thread_id":"019feb52-4b72-7ae3-9282-ff3f7101c36d","root_seq":12,"message_seqs":[12,13],"status":"unknown"},{"thread_id":"019feb64-4990-7ef2-bda1-ecfe708b097b","root_seq":14,"message_seqs":[14,15],"status":"unknown"},{"thread_id":"019feb66-3adb-7272-9578-a3dc217c1144","root_seq":16,"message_seqs":[16,17],"status":"unknown"},{"thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","root_seq":18,"message_seqs":[18,19,20,21],"status":"unknown"},{"thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","root_seq":22,"message_seqs":[22,23,24,25],"status":"unknown"},{"thread_id":"019feb6e-982b-7ad0-b228-492b6d919545","root_seq":26,"message_seqs":[26,27],"status":"unknown"},{"thread_id":"019febb9-b1a6-7b00-9308-e7e09bf2cb8a","root_seq":28,"message_seqs":[28,29],"status":"unknown"}],"turns":[{"turn_id":"019f8d0d-3e7b-7dc0-9574-3e59fa672bd3","state":"completed","started_at":"2026-07-23T03:38:03.261822Z","completed_at":"2026-08-10T12:53:03.609047Z"},{"turn_id":"019fea57-d50c-7d91-88ab-12649bfcff8f","state":"completed","started_at":"2026-08-10T06:24:12.557085Z","completed_at":"2026-08-10T12:53:03.609060Z","thread_id":"019fea57-d50c-7d91-88ab-12649bfcff8f"},{"turn_id":"019fea58-edcd-79e2-b972-0fb3557b6d4e","state":"completed","started_at":"2026-08-10T06:25:24.429439Z","completed_at":"2026-08-10T12:53:03.609050Z","thread_id":"019fea58-edcd-79e2-b972-0fb3557b6d4e"},{"turn_id":"019fea5a-caa0-7b20-b670-3dbd4f8e0c86","state":"completed","started_at":"2026-08-10T06:27:26.497068Z","completed_at":"2026-08-10T12:53:03.609065Z","thread_id":"019fea5a-caa0-7b20-b670-3dbd4f8e0c86"},{"turn_id":"019fea73-7c31-7942-872e-ad847fef2290","state":"completed","started_at":"2026-08-10T06:54:24.822103Z","completed_at":"2026-08-10T12:53:03.609045Z","thread_id":"019fea73-7c31-7942-872e-ad847fef2290"},{"turn_id":"019fea8c-2af2-71d0-a19c-57505af03927","state":"completed","started_at":"2026-08-10T07:21:22.423125Z","completed_at":"2026-08-10T12:53:03.609068Z","thread_id":"019fea8c-2af2-71d0-a19c-57505af03927"},{"turn_id":"019feb4f-efd6-79d2-afdd-9246bfff1528","state":"completed","started_at":"2026-08-10T10:55:12.343288Z","completed_at":"2026-08-10T12:53:03.609063Z","thread_id":"019feb4f-efd6-79d2-afdd-9246bfff1528"},{"turn_id":"019feb52-4b72-7ae3-9282-ff3f7101c36d","state":"completed","started_at":"2026-08-10T10:57:46.866886Z","completed_at":"2026-08-10T12:53:03.609032Z","thread_id":"019feb52-4b72-7ae3-9282-ff3f7101c36d"},{"turn_id":"019feb64-4990-7ef2-bda1-ecfe708b097b","state":"completed","started_at":"2026-08-10T11:17:26.035644Z","completed_at":"2026-08-10T12:53:03.609056Z","thread_id":"019feb64-4990-7ef2-bda1-ecfe708b097b"},{"turn_id":"019feb66-3adb-7272-9578-a3dc217c1144","state":"completed","started_at":"2026-08-10T11:19:33.339717Z","completed_at":"2026-08-10T12:53:03.609053Z","thread_id":"019feb66-3adb-7272-9578-a3dc217c1144"},{"turn_id":"019feb6c-a6b1-7230-8dba-d81223233646","state":"completed","started_at":"2026-08-10T11:26:34.163042Z","completed_at":"2026-08-10T12:53:03.609059Z","thread_id":"019feb6c-a6b1-7230-8dba-d81223233646"},{"turn_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","state":"completed","started_at":"2026-08-10T11:27:31.109938Z","completed_at":"2026-08-10T12:53:03.609037Z","thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5"},{"turn_id":"019feb6e-982b-7ad0-b228-492b6d919545","state":"completed","started_at":"2026-08-10T11:28:41.516444Z","completed_at":"2026-08-10T12:53:03.609041Z","thread_id":"019feb6e-982b-7ad0-b228-492b6d919545"},{"turn_id":"019febb9-b1a6-7b00-9308-e7e09bf2cb8a","state":"completed","started_at":"2026-08-10T12:50:43.245322Z","completed_at":"2026-08-10T12:53:03.609044Z","thread_id":"019febb9-b1a6-7b00-9308-e7e09bf2cb8a"}],"pending_approvals":[],"pending_questions":[],"replayed_envelopes":[],"replayed_tool_envelopes":[{"thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","seq":73,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1191},"turn_id":"019feb6c-a6b1-7230-8dba-d81223233646","payload":{"type":"tool_start","data":{"tool_call_id":"call_00_O5QtedIKMjUJ26ghzqXb4467","name":"get_time"}}},{"thread_id":"019feb6c-a6b1-7230-8dba-d81223233646","seq":74,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1193},"turn_id":"019feb6c-a6b1-7230-8dba-d81223233646","payload":{"type":"tool_end","data":{"tool_call_id":"call_00_O5QtedIKMjUJ26ghzqXb4467","status":"complete","output_preview":"2026-08-10 19:26:37 (UTC+08:00)\nMonday, Local","duration_ms":279}}},{"thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","seq":37,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1340},"turn_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","payload":{"type":"tool_start","data":{"tool_call_id":"call_00_8vMBxHC7OwUsy4ntnNxJ9434","name":"get_weather","arguments_preview":"city: \"San Francisco\""}}},{"thread_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","seq":38,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1342},"turn_id":"019feb6d-8524-7b40-aa60-1a1f69a352b5","payload":{"type":"tool_end","data":{"tool_call_id":"call_00_8vMBxHC7OwUsy4ntnNxJ9434","status":"complete","output_preview":"San Francisco, California, United States\nFoggy\nTemperature: 13.6°C (feels like 12.7°C)\nHumidity: 100%\nWind: 13.9 km/h SW","duration_ms":2916}}}]}"#;

const AGENT_LIST: &str = r#"{"session_id":"alan:local:tui#coding","profile_id":"alan","agents":[]}"#;

const SESSION_GOAL_GET: &str = r#"{"session_id":"alan:local:tui#coding","profile_id":"alan","goal":null}"#;

const LOOP_LIST: &str = r#"{"session_id":"alan:local:tui#coding","profile_id":"alan","loops":[{"loop_id":"loop_01","session_id":"alan:local:tui#coding","profile_id":"alan","prompt":"2 5 seconds","mode":"self_paced","interval_seconds":null,"status":"paused","next_run_at_ms":1784276248509,"last_run_at_ms":null,"expires_at_ms":1784880148509,"created_at_ms":1784275348509,"updated_at_ms":1784275555650},{"loop_id":"loop_02","session_id":"alan:local:tui#coding","profile_id":"alan","prompt":"2","mode":"self_paced","interval_seconds":null,"status":"paused","next_run_at_ms":1784276534444,"last_run_at_ms":null,"expires_at_ms":1784880434444,"created_at_ms":1784275634444,"updated_at_ms":1784275923696}]}"#;

/// The turn the notification stream below was captured from; swapped for the
/// turn_id the live client generates, which every envelope is keyed on.
const CAPTURED_TURN: &str = "019febbb-e207-7630-83e3-b3038fef300e";

/// The notifications one `turn/start` produced, in seq order.
const TURN_STREAM: &str = r#"[{"jsonrpc":"2.0","method":"context/normalization_reported","params":{"session_id":"alan:local:tui#coding","context_state":{"session_id":"alan:local:tui#coding","generation":68,"transcript_hash":"sha256:8e03e5c4dc7727c4626feccc2fd06b1d07b38a755be4ff5d90c6b28614d9d9bb","item_count":47,"token_estimate":1937,"recovery_state":"exact"},"normalization":{"generation":68,"input_transcript_hash":"sha256:8e03e5c4dc7727c4626feccc2fd06b1d07b38a755be4ff5d90c6b28614d9d9bb","output_prompt_hash":"sha256:83228cfba6b56f7e4970bdee254cfe751630f9092d3039ed9f9735f26ce2e4f5","model_capability_id":"deepseek@api/deepseek-v4-pro","prompt_message_count":30,"token_estimate":549,"repaired_count":0,"dropped_count":15,"synthetic_count":0,"truncated_count":0}}},{"jsonrpc":"2.0","method":"progress/updated","params":{"session_id":"alan:local:tui#coding","turn_id":"019febbb-e207-7630-83e3-b3038fef300e","metadata":{"kind":"thinking","iteration":1}}},{"jsonrpc":"2.0","method":"session/orchestration","params":{"session_id":"alan:local:tui#coding","active":true,"running_agents":0,"pending_continuations":0,"phase":"working"}},{"jsonrpc":"2.0","method":"progress/updated","params":{"session_id":"alan:local:tui#coding","turn_id":"019febbb-e207-7630-83e3-b3038fef300e","metadata":{"kind":"response","iteration":1}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":1,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1620},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":"The"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":2,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1621},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" user"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":3,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1622},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" is"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":4,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1623},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" saying"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":5,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1624},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" hello"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":6,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1625},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" again"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":7,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1626},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":"."}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":8,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1627},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" This"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":9,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1628},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" has"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":10,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1629},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" a"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":11,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1630},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" long"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":12,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1631},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" string"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":13,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1632},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":"/testing"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":14,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1633},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" messages"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":15,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1634},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":"."}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":16,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1635},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" keep"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":17,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1636},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" it"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":18,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1637},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" brief"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":19,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1638},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":" friendly"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":20,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1639},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"reasoning_delta","data":{"text":"."}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":21,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1640},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":"Hello","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":22,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1641},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":"!","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":23,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1642},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" 😊","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":24,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1643},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" Still","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":25,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1644},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" here","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":26,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1645},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":".","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":27,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1646},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" Anything","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":28,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1647},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" you","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":29,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1648},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" like","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":30,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1649},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" to","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":31,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1650},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" do","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":32,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1651},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":" today","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":33,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1652},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_delta","data":{"text":"?","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1"}}}},{"jsonrpc":"2.0","method":"progress/updated","params":{"session_id":"alan:local:tui#coding","turn_id":"019febbb-e207-7630-83e3-b3038fef300e","metadata":{"kind":"stream_end"}}},{"jsonrpc":"2.0","method":"progress/updated","params":{"session_id":"alan:local:tui#coding","turn_id":"019febbb-e207-7630-83e3-b3038fef300e","metadata":{"kind":"token_cost_update","token_cost":{"input_tokens":711050,"output_tokens":44435,"session_cost":0.21267676000000008,"model":"deepseek-v4-pro","context_window":1048576}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":34,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1653},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"user_message","data":{"text":"hello"}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":35,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1654},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"assistant_persisted","data":{"text":"Hello! 😊 Still here. Anything you'd like to do today?","assistant_segment_id":"019febbb-e207-7630-83e3-b3038fef300e:assistant:1","meta":{"message_id":"alan:local:tui#coding:31:1786366391331758000","persisted_at":"2026-08-10T12:53:11.331758Z"}}}}},{"jsonrpc":"2.0","method":"projection/envelope","params":{"session_id":"alan:local:tui","topic":"coding","thread_id":"019febbb-e207-7630-83e3-b3038fef300e","seq":36,"cursor":{"stream":"alan:local:tui#coding\u0000~cwd-74427c29e62e0989","seq":1655},"turn_id":"019febbb-e207-7630-83e3-b3038fef300e","payload":{"type":"turn_terminal","data":{"outcome":"completed","token_usage":{"input_tokens":86,"output_tokens":42}}}}},{"jsonrpc":"2.0","method":"session/orchestration","params":{"session_id":"alan:local:tui#coding","active":false,"running_agents":0,"pending_continuations":0}}]"#;

/// The `result` body recorded for `method`. `None` means the capture never saw
/// one — and with no upstream there is nobody else to ask, so the caller turns
/// that into an explicit error rather than leaving the client waiting forever.
fn canned(method: &str) -> Option<&'static str> {
    match method {
        "config/capabilities/list" => Some(CAPABILITIES),
        "profile/llm/list" => Some(PROFILE_LLM_LIST),
        "session/status/read" => Some(SESSION_STATUS_READ),
        "session/hydrate" => Some(SESSION_HYDRATE),
        "agent/list" => Some(AGENT_LIST),
        "session/goal/get" => Some(SESSION_GOAL_GET),
        "loop/list" => Some(LOOP_LIST),
        _ => None,
    }
}

/// Point a canned body at the session the live client actually opened.
///
/// Two forms need swapping: request results carry the full `<profile>:<channel>
/// :<chat>#<topic>` key, while `projection/envelope` splits it into `session_id`
/// without the topic plus a separate `topic` field. Replacing the long form
/// first leaves only genuine base-form occurrences for the second pass.
fn retarget(body: &str) -> String {
    let session = SESSION.lock().unwrap();
    if session.is_empty() || *session == CAPTURED_SESSION {
        return body.to_string();
    }
    let captured_base = CAPTURED_SESSION.split('#').next().unwrap_or(CAPTURED_SESSION);
    let live_base = session.split('#').next().unwrap_or(&session);
    body.replace(CAPTURED_SESSION, &session)
        .replace(captured_base, live_base)
}

/// `session/open` returns `{"opened": SessionOpened}`. The capture never
/// recorded one — the reply is ~55 KB and the fiddler forwarded it as split
/// bytes without decoding — so it is assembled from the pieces that WERE
/// captured, every one of them lifted from `session/status/read` or
/// `session/hydrate` rather than invented.
fn session_open_result() -> Option<Value> {
    let capabilities: Value = serde_json::from_str(&retarget(CAPABILITIES)).ok()?;
    let status: Value = serde_json::from_str(&retarget(SESSION_STATUS_READ)).ok()?;
    let hydrate: Value = serde_json::from_str(&retarget(SESSION_HYDRATE)).ok()?;
    Some(serde_json::json!({"opened": {
        "session_id": status["session_id"],
        "active_profile_id": status["profile_id"],
        "workspace_root": status["runtime_policy_stamp"]["workspace_root"],
        "context": status["context"],
        "context_state": status["context_state"],
        "cursor": hydrate["cursor"],
        "capabilities": capabilities["capabilities"],
    }}))
}

/// The profile a request is scoped to, falling back to the captured one.
fn profile_of(params: &Value) -> String {
    params
        .get("profile_id")
        .and_then(Value::as_str)
        .unwrap_or(CAPTURED_PROFILE)
        .to_string()
}

/// The session to stamp on a reply: whichever one the client is using, or the
/// captured one until it has told us. Takes and releases the lock, because
/// callers go on to `retarget`, which takes it again.
fn live_session() -> String {
    let session = SESSION.lock().unwrap();
    if session.is_empty() {
        CAPTURED_SESSION.to_string()
    } else {
        session.clone()
    }
}

/// The result body for `method`, or None when nothing was captured.
fn result_for(method: &str, params: &Value) -> Option<Value> {
    match method {
        //`turn/start` is acknowledged, not answered: the turn itself arrives as
        //the notification stream that `turn_stream` replays.
        "turn/start" => Some(serde_json::json!({"accepted": true})),
        "session/open" => session_open_result(),
        //The capture recorded no body, but the client's next move gives the
        //decision away: it went straight to `session/open` on
        //"alan:local:tui#coding" rather than prompting. Only `resume` does
        //that, and the client builds that id out of `resolved_profile`
        //(`open_resolved_launch_session`) — so echoing the profile the client
        //asked for is what keeps the session it opens consistent with the one
        //the canned bodies are retargeted to. `existing_profiles` belongs to
        //`cross_profile` and is omitted.
        "launch/resolve" => Some(serde_json::json!({
            "decision": "resume",
            "resolved_profile": profile_of(params),
        })),
        //Neither was captured, but the capture still fixes every value: the
        //session had `mcp_summary {0,0,0,0}` and `runtime_policy_stamp
        //.mcp_servers []`, so there are no servers to report.
        "mcp/status/list" => Some(serde_json::json!({
            "profile_id": profile_of(params),
            "session_id": live_session(),
            "servers": [],
            "summary": {"connected": 0, "connecting": 0, "failed": 0, "disabled": 0},
        })),
        //Likewise `tool_summary {visible 0, enabled 0, denied 0, policy_id
        //"profile"}`. The server builds `tools` by filtering its spec table to
        //names that are visible or disabled, and both sets were empty here —
        //this session defers everything behind `dynamic_tool_discovery`, and
        //deferred names are deliberately not listed.
        //
        //`coding_tool_contract` is omitted. The client types it `Option`, so
        //absence is benign, whereas its contents derive entirely from
        //server-side registry state the capture never exposed: synthesising it
        //from an empty toolset would mark every required tool missing and
        //report `status: "incomplete"`, i.e. claim a broken session.
        "tool/status/list" => Some(serde_json::json!({
            "profile_id": profile_of(params),
            "session_id": live_session(),
            "policy_id": "profile",
            "tools": [],
        })),
        _ => serde_json::from_str(&retarget(canned(method)?)).ok(),
    }
}

/// The pushed notifications a turn produces, retargeted at the live session,
/// turn and prompt.
///
/// The client tracks envelope `seq` and reports gaps as `protocol/replay_lossy`,
/// so these are renumbered contiguously: the capture is missing seven deltas
/// (the fiddler forwards payloads it cannot frame-align without recording them),
/// and replaying its original numbering would look like loss on every turn. The
/// text those deltas carried is gone, so the streamed reasoning reads slightly
/// clipped — `assistant_persisted` still carries the complete final answer,
/// which is what the client keeps.
fn turn_stream(turn_id: &str, prompt: &str) -> Vec<Value> {
    let raw = retarget(TURN_STREAM).replace(CAPTURED_TURN, turn_id);
    let mut envelopes: Vec<Value> = match serde_json::from_str(&raw) {
        Ok(envelopes) => envelopes,
        Err(_) => return vec![],
    };
    for envelope in &mut envelopes {
        if envelope.pointer("/params/payload/type").and_then(Value::as_str) == Some("user_message") {
            if let Some(text) = envelope.pointer_mut("/params/payload/data/text") {
                *text = Value::String(prompt.to_string());
            }
        }
    }
    envelopes
}

/// Concatenate the text parts of a `turn/start` `input` array.
fn prompt_of(params: &Value) -> String {
    params
        .get("input")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Every frame to send back for one client frame: the reply first, then any
/// notifications that request sets off. Empty when the frame is not a JSON-RPC
/// request — a client notification has no id and expects no answer.
fn replies_to(text: &str) -> Vec<String> {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let method = match v.get("method").and_then(Value::as_str) {
        Some(method) => method.to_string(),
        None => return vec![],
    };
    if let Some(session) = v.pointer("/params/session_id").and_then(Value::as_str) {
        *SESSION.lock().unwrap() = session.to_string();
    }
    let id = match v.get("id") {
        Some(id) => id,
        None => return vec![],
    };

    let params = v.get("params").cloned().unwrap_or(Value::Null);
    let mut out = vec![match result_for(&method, &params) {
        Some(result) => serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}),
        //Answering is not optional now that nothing sits behind this guest: a
        //silent drop looks identical to a hung server, and the TUI waits on the
        //id forever. Name the method so the gap is obvious in the client's own
        //error surface.
        None => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": format!("mock_octos: no response captured for {}", method),
            }
        }),
    }
    .to_string()];

    if method == "turn/start" {
        let turn_id = params.get("turn_id").and_then(Value::as_str).unwrap_or(CAPTURED_TURN);
        out.extend(
            turn_stream(turn_id, &prompt_of(&params))
                .iter()
                .map(Value::to_string),
        );
    }
    out
}

/// The 101 the real server would have sent. `Sec-WebSocket-Accept` is derived
/// from the client's key, so this cannot be a canned string.
fn handshake_response(raw: &[u8]) -> Option<String> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    if !request.parse(raw).ok()?.is_complete() {
        return None;
    }
    let lookup = |name: &'static str| -> Option<&str> {
        request
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .and_then(|header| std::str::from_utf8(header.value).ok())
    };
    let accept = websocket_codec::ClientRequest::parse(lookup).ok()?.ws_accept();
    Some(format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept
    ))
}

/// Write frames back down the client's own connection.
///
/// `TcpResponse` (`tcpproxy/pool.go`) picks the local connection by
/// `strings.Contains(key, portmap)` and then `lconn.RemoteAddr() == req.Laddr`,
/// so the binding must be the port map and `Laddr` must be the address the host
/// handed us — an empty `Laddr` broadcasts to every client on the port.
///
/// Called through `host_call` rather than `foo_tcp_response!`: the host decodes
/// this payload as a msgpack ARRAY of `TcpReq` and answers with zero bytes,
/// while the macro serialises a single struct and then tries to parse a
/// `TcpReq` back out of the empty reply.
fn send_to_client(items: Vec<TcpReq>) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    if items.is_empty() {
        return Ok(());
    }
    let buf = rmp_serde::to_vec(&items)?;
    host_call(PORT_MAP, "foo", "tcp_response", &buf)?;
    Ok(())
}

fn frame_item(payload: Vec<u8>, description: String, laddr: &str, raddr: &str) -> TcpReq {
    TcpReq {
        Payload: general_purpose::STANDARD.encode(payload),
        String: description,
        Index: 0,
        //A non-empty Id makes the host treat this as an RPC awaiting a matching
        //reply from a remote that no longer exists.
        Id: String::new(),
        Command: String::new(),
        ReportType: String::from("Mock"),
        Timeout: false,
        Laddr: laddr.to_string(),
        Raddr: raddr.to_string(),
    }
}

/// Everything the client sends is answered here; nothing is ever forwarded.
fn _req(msg: &[u8]) -> CallResult {
    let tcp_payload: TcpPayload = rmp_serde::from_read_ref(msg)?;
    let payload = general_purpose::STANDARD.decode(tcp_payload.Payload.clone())?;
    let conn = format!("{}-{}", tcp_payload.Laddr, tcp_payload.Raddr);
    let mut channels = CHANNELS.lock().unwrap();
    let channel = channels.entry(conn).or_insert_with(Conn::new);

    //A payload opening with a request line is a fresh connection: the OS reuses
    //ephemeral ports, so without re-arming, a new client inherits a channel
    //already past its handshake and its upgrade is fed to the frame decoder.
    if payload.starts_with(b"GET ") {
        *channel = Conn::new();
        let items = match handshake_response(&payload) {
            Some(response) => vec![frame_item(
                response.clone().into_bytes(),
                response,
                &tcp_payload.Laddr,
                &tcp_payload.Raddr,
            )],
            //Header split across TCP reads: wait for the rest rather than
            //answering a handshake we have not finished reading.
            None => return Ok(b"/continue".to_vec()),
        };
        channel.upgraded = true;
        drop(channels);
        send_to_client(items)?;
        return Ok(b"/continue".to_vec());
    }
    if !channel.upgraded {
        return Ok(b"/continue".to_vec());
    }

    channel.buf.extend_from_slice(&payload);
    let mut items = vec![];
    //Drain every complete frame: one TCP read routinely carries several, and
    //answering only the first strands the rest of the client's requests.
    //`decode` consumes nothing on a partial frame, so a split tail simply stays
    //buffered for the next payload.
    while let Ok(Some(message)) = channel.decoder.decode(&mut channel.buf) {
        //One request can produce many frames: a turn is a reply followed by the
        //whole notification stream.
        let outgoing: Vec<websocket_codec::Message> = match message.opcode() {
            //Text is the only opcode carrying JSON-RPC.
            Opcode::Text => message
                .as_text()
                .map(replies_to)
                .unwrap_or_default()
                .into_iter()
                .map(websocket_codec::Message::text)
                .collect(),
            //Nothing upstream is going to keep this connection alive any more.
            Opcode::Ping => vec![websocket_codec::Message::pong(message.data().clone())],
            Opcode::Close => vec![websocket_codec::Message::close()],
            _ => continue,
        };
        for frame in outgoing {
            let mut encoded = BytesMut::new();
            if channel.encoder.encode(frame.clone(), &mut encoded).is_err() {
                continue;
            }
            items.push(frame_item(
                encoded.to_vec(),
                frame.as_text().unwrap_or_default().to_string(),
                &tcp_payload.Laddr,
                &tcp_payload.Raddr,
            ));
        }
    }
    drop(channels);
    send_to_client(items)?;
    //Never forward: "/continue" is what tells the host to write nothing to the
    //remote (`if !bytes.Equal(mb, "/continue")` in tcpproxy/pool.go).
    Ok(b"/continue".to_vec())
}

/// The remote is a placeholder that is never written to, so this only ever
/// fires if something else answers on that port. Drop it either way.
fn _res(_msg: &[u8]) -> CallResult {
    Ok(b"/continue".to_vec())
}

#[no_mangle]
pub extern "C" fn _start() {
    register_function(concat!(port_map!(), "_tcp_modify_req"), _req);
    register_function(concat!(port_map!(), "_tcp_modify_res"), _res);
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    lazy_static! {
        static ref TEST_LOCK: Mutex<()> = Mutex::new(());
    }

    /// SESSION is process-wide, exactly as it is in the guest, so the tests take
    /// turns rather than racing each other's state.
    fn reset() -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        CHANNELS.lock().unwrap().clear();
        SESSION.lock().unwrap().clear();
        guard
    }

    /// The single reply to a request that produces no notifications.
    fn reply(request: &str) -> Value {
        let frames = replies_to(request);
        assert_eq!(frames.len(), 1, "expected exactly one frame");
        serde_json::from_str(&frames[0]).expect("valid json")
    }

    fn frames(request: &str) -> Vec<Value> {
        replies_to(request)
            .iter()
            .map(|frame| serde_json::from_str(frame).expect("valid json"))
            .collect()
    }

    /// Every captured method answers from its own recorded body. The capture
    /// pairs a reply with whatever request was in flight rather than the one it
    /// answers, so this is keyed on the method, never on that adjacency.
    #[test]
    fn answers_each_captured_method_from_its_own_body() {
        let _guard = reset();
        assert!(reply(r#"{"jsonrpc":"2.0","id":1,"method":"config/capabilities/list","params":{}}"#)
            ["result"]["capabilities"]
            .is_object());
        assert_eq!(
            reply(r#"{"jsonrpc":"2.0","id":2,"method":"profile/llm/list","params":{}}"#)["result"]["primary"]
                ["model"],
            "deepseek-v4-pro"
        );
        assert_eq!(
            reply(r#"{"jsonrpc":"2.0","id":3,"method":"loop/list","params":{}}"#)["result"]["loops"]
                .as_array()
                .expect("loops")
                .len(),
            2
        );
        assert_eq!(
            reply(r#"{"jsonrpc":"2.0","id":4,"method":"session/hydrate","params":{}}"#)["result"]["messages"]
                .as_array()
                .expect("messages")
                .len(),
            30
        );
    }

    /// The id is echoed verbatim — the client matches replies on it — and it is
    /// the live one, not the `tui-N` the capture happened to use.
    #[test]
    fn echoes_the_live_id() {
        let _guard = reset();
        let answer = reply(r#"{"jsonrpc":"2.0","id":"tui-99","method":"agent/list","params":{}}"#);
        assert_eq!(answer["id"], "tui-99");
        assert_eq!(answer["jsonrpc"], "2.0");
    }

    /// With no upstream, dropping an uncaptured method is indistinguishable
    /// from a hung server: the client waits on that id forever.
    #[test]
    fn errors_rather_than_hanging_on_an_uncaptured_method() {
        let _guard = reset();
        let answer = reply(r#"{"jsonrpc":"2.0","id":7,"method":"snapshot/list","params":{}}"#);
        assert!(answer.get("result").is_none());
        assert_eq!(answer["error"]["code"], -32000);
        assert!(answer["error"]["message"].as_str().expect("message").contains("snapshot/list"));
    }

    /// The client decodes into `McpStatusListResult` / `ToolStatusListResult`
    /// and turns any decode failure into a visible app error, so the shapes
    /// matter more than the contents. `session_id` is the one required member
    /// of each.
    #[test]
    fn reports_an_empty_mcp_and_tool_surface_for_the_live_session() {
        let _guard = reset();
        let session = r#"{"session_id":"bob:local:tui#coding","profile_id":"bob"}"#;
        let mcp = reply(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"mcp/status/list","params":{}}}"#,
            session
        ))["result"]
            .clone();
        assert_eq!(mcp["session_id"], "bob:local:tui#coding");
        assert_eq!(mcp["profile_id"], "bob");
        assert_eq!(mcp["servers"].as_array().expect("servers").len(), 0);
        // Corroborated by mcp_summary in the captured session/status/read.
        assert_eq!(mcp["summary"], serde_json::json!({"connected":0,"connecting":0,"failed":0,"disabled":0}));

        let tools = reply(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"tool/status/list","params":{}}}"#,
            session
        ))["result"]
            .clone();
        assert_eq!(tools["session_id"], "bob:local:tui#coding");
        // Corroborated by tool_summary.policy_id in the same capture.
        assert_eq!(tools["policy_id"], "profile");
        assert_eq!(tools["tools"].as_array().expect("tools").len(), 0);
        assert!(
            tools.get("coding_tool_contract").is_none(),
            "an invented contract would report the session broken"
        );
    }

    /// Only `resume` sends the client straight to `session/open`; the other
    /// three decisions open a prompt menu or bounce it to onboarding, which is
    /// not what the capture shows happening next.
    #[test]
    fn resolves_launch_to_resume() {
        let _guard = reset();
        let result = reply(
            r#"{"jsonrpc":"2.0","id":3,"method":"launch/resolve","params":{"cwd":"/tmp/x","profile_id":"alan"}}"#,
        )["result"]
            .clone();
        assert_eq!(result["decision"], "resume");
        assert_eq!(result["resolved_profile"], "alan");
        assert!(result.get("existing_profiles").is_none(), "cross_profile only");
    }

    /// The client derives its session id from `resolved_profile`
    /// (`open_resolved_launch_session`), so echoing back a different profile
    /// than the one asked for would send it to open a session the canned
    /// bodies are not keyed to.
    #[test]
    fn echoes_the_requested_profile_and_falls_back_to_the_captured_one() {
        let _guard = reset();
        assert_eq!(
            reply(r#"{"jsonrpc":"2.0","id":3,"method":"launch/resolve","params":{"cwd":"/tmp/x","profile_id":"bob"}}"#)
                ["result"]["resolved_profile"],
            "bob"
        );
        assert_eq!(
            reply(r#"{"jsonrpc":"2.0","id":3,"method":"launch/resolve","params":{"cwd":"/tmp/x"}}"#)["result"]
                ["resolved_profile"],
            CAPTURED_PROFILE
        );
    }

    /// `session/open` returns `{"opened": SessionOpened}`; `session_id` is the
    /// only member the client requires, but an empty shell would leave it with
    /// no cursor to resume from and no negotiated capabilities.
    #[test]
    fn opens_a_session_with_the_pieces_the_capture_did_record() {
        let _guard = reset();
        let opened = reply(
            r#"{"jsonrpc":"2.0","id":4,"method":"session/open","params":{"session_id":"bob:local:tui#coding"}}"#,
        )["result"]["opened"]
            .clone();
        assert_eq!(opened["session_id"], "bob:local:tui#coding");
        assert_eq!(opened["active_profile_id"], "alan");
        assert_eq!(opened["cursor"]["seq"], 1613);
        assert_eq!(opened["context_state"]["recovery_state"], "exact");
        assert_eq!(
            opened["capabilities"]["supported_methods"].as_array().expect("methods").len(),
            67
        );
        assert!(opened["workspace_root"].is_string());
    }

    /// A turn is an ack plus the whole notification stream. The client keys
    /// every envelope on its own turn_id, so replaying the captured one would
    /// have the TUI discard the lot.
    #[test]
    fn replays_a_turn_against_the_live_turn_id_and_prompt() {
        let _guard = reset();
        let frames = frames(
            r#"{"jsonrpc":"2.0","id":14,"method":"turn/start","params":{"session_id":"bob:local:tui#coding","turn_id":"live-turn-1","input":[{"kind":"text","text":"how are you"}]}}"#,
        );
        assert_eq!(frames[0]["result"]["accepted"], true);
        assert!(frames.len() > 40, "got {} frames", frames.len());

        let joined = serde_json::to_string(&frames).expect("json");
        assert!(!joined.contains(CAPTURED_TURN), "captured turn id leaked");
        assert!(!joined.contains(CAPTURED_SESSION), "captured session id leaked");

        let payloads: Vec<&str> = frames
            .iter()
            .filter_map(|frame| frame.pointer("/params/payload/type").and_then(Value::as_str))
            .collect();
        assert!(payloads.contains(&"reasoning_delta"));
        assert!(payloads.contains(&"assistant_delta"));
        assert_eq!(payloads.last(), Some(&"turn_terminal"), "the turn must end terminally");

        // The prompt is the user's, not the captured "hello".
        let echoed = frames
            .iter()
            .find(|frame| frame.pointer("/params/payload/type").and_then(Value::as_str) == Some("user_message"))
            .expect("user_message");
        assert_eq!(echoed["params"]["payload"]["data"]["text"], "how are you");
    }

    /// The client reports gaps in envelope seq as replay loss, and the capture
    /// is missing seven deltas, so the stream is renumbered contiguously.
    #[test]
    fn numbers_envelope_seqs_without_gaps() {
        let _guard = reset();
        let frames = frames(
            r#"{"jsonrpc":"2.0","id":14,"method":"turn/start","params":{"turn_id":"live-turn-1","input":[]}}"#,
        );
        let seqs: Vec<u64> = frames
            .iter()
            .filter_map(|frame| frame.pointer("/params/seq").and_then(Value::as_u64))
            .collect();
        assert!(!seqs.is_empty());
        assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>());
    }

    /// The final answer is what the client keeps once the deltas stop, so it
    /// has to be the complete sentence even though some deltas were not
    /// captured.
    #[test]
    fn persists_the_complete_answer_even_though_deltas_are_lossy() {
        let _guard = reset();
        let frames = frames(
            r#"{"jsonrpc":"2.0","id":14,"method":"turn/start","params":{"turn_id":"t","input":[]}}"#,
        );
        let persisted = frames
            .iter()
            .find(|frame| {
                frame.pointer("/params/payload/type").and_then(Value::as_str) == Some("assistant_persisted")
            })
            .expect("assistant_persisted");
        assert_eq!(
            persisted["params"]["payload"]["data"]["text"],
            "Hello! \u{1F60A} Still here. Anything you'd like to do today?"
        );
    }

    /// A client notification carries no id and expects no answer; replying to
    /// one would put an unsolicited frame on the wire.
    #[test]
    fn stays_quiet_for_notifications() {
        let _guard = reset();
        assert!(replies_to(r#"{"jsonrpc":"2.0","method":"turn/interrupt","params":{}}"#).is_empty());
    }

    /// Canned bodies embed the captured session in dozens of places, including
    /// every message_id.
    #[test]
    fn retargets_canned_bodies_at_the_live_session() {
        let _guard = reset();
        let answer = reply(
            r#"{"jsonrpc":"2.0","id":6,"method":"session/hydrate","params":{"session_id":"bob:local:tui#coding"}}"#,
        );
        assert_eq!(answer["result"]["session_id"], "bob:local:tui#coding");
        assert!(
            !serde_json::to_string(&answer).unwrap().contains(CAPTURED_SESSION),
            "no captured session id survives retargeting"
        );
    }

    /// The accept value is derived from the client's key, so a canned 101 would
    /// be rejected by any conforming client.
    #[test]
    fn derives_the_handshake_accept_from_the_clients_key() {
        let _guard = reset();
        let request = b"GET /api/ui-protocol/ws HTTP/1.1\r\nHost: 127.0.0.1:3335\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: wThjiiUQ0W/uJEO+1cCujA==\r\n\r\n";
        let response = handshake_response(request).expect("handshake");
        assert!(response.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        // base64(sha1("wThjiiUQ0W/uJEO+1cCujA==" + RFC6455 GUID))
        assert!(
            response.contains("Sec-WebSocket-Accept: EiOE8cESg3a8QQmX8+xmdGXFu6k="),
            "got {}",
            response
        );
    }

    /// A truncated header must not be answered: the accept value would be
    /// computed from a key that has not fully arrived.
    #[test]
    fn waits_for_a_handshake_split_across_reads() {
        let _guard = reset();
        assert!(handshake_response(b"GET /api/ui-protocol/ws HTTP/1.1\r\nHost: 127.0.0.1").is_none());
    }

    /// Client frames are masked and server frames are not; encoding a reply
    /// with the request codec would produce a frame the client discards.
    #[test]
    fn replies_with_unmasked_frames_that_round_trip() {
        let _guard = reset();
        let mut client = MessageCodec::client();
        let mut request = BytesMut::new();
        client
            .encode(
                websocket_codec::Message::text(
                    r#"{"jsonrpc":"2.0","id":"tui-1","method":"agent/list","params":{}}"#,
                ),
                &mut request,
            )
            .expect("encodes");
        assert!(request[1] & 0x80 != 0, "client frames are masked");

        let mut conn = Conn::new();
        conn.buf.extend_from_slice(&request);
        let message = conn.decoder.decode(&mut conn.buf).expect("decodes").expect("a frame");
        let answer = replies_to(message.as_text().expect("text")).remove(0);

        let mut encoded = BytesMut::new();
        conn.encoder
            .encode(websocket_codec::Message::text(answer), &mut encoded)
            .expect("encodes");
        assert!(encoded[1] & 0x80 == 0, "server frames are not masked");

        let decoded = MessageCodec::server()
            .decode(&mut encoded)
            .expect("decodes")
            .expect("a frame");
        let value: Value = serde_json::from_str(decoded.as_text().expect("text")).expect("json");
        assert_eq!(value["id"], "tui-1");
        assert_eq!(value["result"]["agents"].as_array().expect("agents").len(), 0);
    }
}
