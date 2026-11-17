use serde_json::{json, Value};

use crate::model::NormEvent;
use crate::timefmt::{now_rfc3339, parse_timestamp};

/// Parse one complete log line into zero or more normalized events.
///
/// `stream_event` partials from Claude Code are skipped. A line that is not
/// JSON becomes a single message, except an empty line, which is ignored.
pub fn parse_line(agent: &str, line: &str, source: &str, seq: &mut u64) -> Vec<NormEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let fallback = now_rfc3339();
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return vec![event(
            agent,
            &fallback,
            "message",
            clip(trimmed, 180),
            json!({ "text": trimmed }),
            source,
            seq,
        )];
    };
    let ts = timestamp_of(&value, &fallback);
    let mut out = Vec::new();
    match agent {
        "claude" => parse_claude(&value, &ts, source, seq, &mut out),
        "cursor" => parse_cursor(&value, &ts, source, seq, &mut out),
        "codex" => parse_codex(&value, &ts, source, seq, &mut out),
        "graphify" => parse_graphify(&value, &ts, source, seq, &mut out),
        "orca" => parse_orca(&value, &ts, source, seq, &mut out),
        _ => out.push(event(
            agent,
            &ts,
            "message",
            clip(trimmed, 180),
            json!({ "text": trimmed }),
            source,
            seq,
        )),
    }
    if out.is_empty() && value.get("type").and_then(Value::as_str) != Some("stream_event") {
        out.push(event(
            agent,
            &ts,
            "status",
            clip(&compact(&value), 180),
            json!({ "raw": value }),
            source,
            seq,
        ));
    }
    out
}

fn parse_claude(value: &Value, ts: &str, source: &str, seq: &mut u64, out: &mut Vec<NormEvent>) {
    match value.get("type").and_then(Value::as_str) {
        Some("stream_event") => {}
        Some("system") => {
            let subtype = text_at(value, "subtype").unwrap_or("status");
            out.push(event(
                "claude",
                ts,
                "status",
                format!("session {subtype}"),
                json!({ "subtype": subtype }),
                source,
                seq,
            ));
        }
        Some("assistant") => {
            if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                for block in blocks {
                    push_content_block("claude", block, ts, source, seq, out);
                }
            }
        }
        Some("user") => {
            if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let output = content_text(block.get("content"));
                        let errored = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        out.push(event(
                            "claude",
                            ts,
                            if errored { "error" } else { "tool_result" },
                            clip(&output, 180),
                            json!({
                                "tool_use_id": block.get("tool_use_id"),
                                "output": output,
                            }),
                            source,
                            seq,
                        ));
                    }
                }
            }
        }
        Some("result") => {
            let is_error = value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || value.get("subtype").and_then(Value::as_str) == Some("error");
            let result = text_at(value, "result").unwrap_or("");
            out.push(event(
                "claude",
                ts,
                if is_error { "error" } else { "status" },
                if result.is_empty() {
                    if is_error {
                        "run failed"
                    } else {
                        "run finished"
                    }
                    .to_string()
                } else {
                    clip(result, 180)
                },
                json!({ "result": result, "subtype": value.get("subtype") }),
                source,
                seq,
            ));
        }
        Some("rate_limit_event") => out.push(event(
            "claude",
            ts,
            "status",
            "rate limit".to_string(),
            json!({ "raw": value }),
            source,
            seq,
        )),
        _ => {}
    }
}

fn parse_cursor(value: &Value, ts: &str, source: &str, seq: &mut u64, out: &mut Vec<NormEvent>) {
    match value.get("type").and_then(Value::as_str) {
        Some("system") => {
            let subtype = text_at(value, "subtype").unwrap_or("status");
            out.push(event(
                "cursor",
                ts,
                "status",
                format!("session {subtype}"),
                json!({ "subtype": subtype }),
                source,
                seq,
            ));
        }
        Some("assistant") | Some("user") => {
            if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                for block in blocks {
                    push_content_block("cursor", block, ts, source, seq, out);
                }
            }
        }
        Some("tool_call") => {
            let subtype = text_at(value, "subtype").unwrap_or("started");
            let (tool, input, output) = cursor_tool(value.get("tool_call"));
            let kind = if subtype == "completed" {
                "tool_result"
            } else {
                "tool_call"
            };
            let summary = cursor_summary(&tool, &input, &output);
            out.push(event(
                "cursor",
                ts,
                kind,
                summary,
                json!({ "tool": tool, "input": input, "output": output, "subtype": subtype }),
                source,
                seq,
            ));
        }
        Some("result") => {
            let is_error = value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || value.get("subtype").and_then(Value::as_str) == Some("error");
            let result = text_at(value, "result").unwrap_or("");
            out.push(event(
                "cursor",
                ts,
                if is_error { "error" } else { "status" },
                if result.is_empty() {
                    "run finished".to_string()
                } else {
                    clip(result, 180)
                },
                json!({ "result": result }),
                source,
                seq,
            ));
        }
        _ => {}
    }
}

fn parse_codex(value: &Value, ts: &str, source: &str, seq: &mut u64, out: &mut Vec<NormEvent>) {
    if let Some(message) = value.get("msg") {
        parse_codex_legacy(message, ts, source, seq, out);
        return;
    }
    match value.get("type").and_then(Value::as_str) {
        Some("thread.started") | Some("turn.started") | Some("turn.completed") => {
            let kind = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("status");
            out.push(event(
                "codex",
                ts,
                "status",
                kind.replace('.', " "),
                json!({ "type": kind }),
                source,
                seq,
            ));
        }
        Some("error") => {
            let message = text_at(value, "message").unwrap_or("error");
            out.push(event(
                "codex",
                ts,
                "error",
                clip(message, 180),
                json!({ "message": message }),
                source,
                seq,
            ));
        }
        Some("item.started") | Some("item.completed") => {
            let phase = value.get("type").and_then(Value::as_str).unwrap_or("");
            if let Some(item) = value.get("item") {
                push_codex_item(item, phase, ts, source, seq, out);
            }
        }
        _ => {}
    }
}

fn parse_codex_legacy(
    message: &Value,
    ts: &str,
    source: &str,
    seq: &mut u64,
    out: &mut Vec<NormEvent>,
) {
    match message.get("type").and_then(Value::as_str) {
        Some("task_started") | Some("task_complete") => {
            let text = text_at(message, "last_agent_message").unwrap_or("task");
            out.push(event(
                "codex",
                ts,
                "status",
                clip(text, 180),
                json!({ "raw": message }),
                source,
                seq,
            ));
        }
        Some("agent_message") => {
            let text = text_at(message, "message").unwrap_or("");
            out.push(event(
                "codex",
                ts,
                "message",
                clip(text, 180),
                json!({ "text": text }),
                source,
                seq,
            ));
        }
        Some("agent_reasoning") => {
            let text = text_at(message, "text").unwrap_or("");
            out.push(event(
                "codex",
                ts,
                "thinking",
                clip(text, 180),
                json!({ "text": text }),
                source,
                seq,
            ));
        }
        Some("exec_command_begin") => {
            let command = command_text(message.get("command"));
            out.push(event(
                "codex",
                ts,
                "tool_call",
                clip(&command, 180),
                json!({ "tool": "command", "input": command }),
                source,
                seq,
            ));
        }
        Some("exec_command_end") => {
            let output = text_at(message, "stdout").unwrap_or("");
            let exit = message.get("exit_code").cloned().unwrap_or(Value::Null);
            let errored = exit.as_i64().unwrap_or(0) != 0;
            out.push(event(
                "codex",
                ts,
                if errored { "error" } else { "tool_result" },
                clip(output, 180),
                json!({ "output": output, "exit_code": exit }),
                source,
                seq,
            ));
        }
        Some("error") => {
            let text = text_at(message, "message").unwrap_or("error");
            out.push(event(
                "codex",
                ts,
                "error",
                clip(text, 180),
                json!({ "message": text }),
                source,
                seq,
            ));
        }
        _ => {}
    }
}

fn push_codex_item(
    item: &Value,
    phase: &str,
    ts: &str,
    source: &str,
    seq: &mut u64,
    out: &mut Vec<NormEvent>,
) {
    match item.get("type").and_then(Value::as_str) {
        Some("reasoning") => {
            if phase == "item.completed" {
                let text = text_at(item, "text").unwrap_or("");
                out.push(event(
                    "codex",
                    ts,
                    "thinking",
                    clip(text, 180),
                    json!({ "text": text }),
                    source,
                    seq,
                ));
            }
        }
        Some("agent_message") => {
            if phase == "item.completed" {
                let text = text_at(item, "text").unwrap_or("");
                out.push(event(
                    "codex",
                    ts,
                    "message",
                    clip(text, 180),
                    json!({ "text": text }),
                    source,
                    seq,
                ));
            }
        }
        Some("command_execution") => {
            let command = text_at(item, "command").unwrap_or("command");
            if phase == "item.started" {
                out.push(event(
                    "codex",
                    ts,
                    "tool_call",
                    clip(command, 180),
                    json!({ "tool": "command", "input": command }),
                    source,
                    seq,
                ));
            } else {
                let output = text_at(item, "aggregated_output").unwrap_or("");
                let exit = item.get("exit_code").cloned().unwrap_or(Value::Null);
                let errored = exit.as_i64().unwrap_or(0) != 0;
                out.push(event(
                    "codex",
                    ts,
                    if errored { "error" } else { "tool_result" },
                    if output.is_empty() { clip(command, 180) } else { clip(output, 180) },
                    json!({ "tool": "command", "input": command, "output": output, "exit_code": exit }),
                    source,
                    seq,
                ));
            }
        }
        Some("error") => {
            let text = text_at(item, "message").unwrap_or("error");
            out.push(event(
                "codex",
                ts,
                "error",
                clip(text, 180),
                json!({ "message": text }),
                source,
                seq,
            ));
        }
        _ => {}
    }
}

fn parse_graphify(value: &Value, ts: &str, source: &str, seq: &mut u64, out: &mut Vec<NormEvent>) {
    match value.get("type").and_then(Value::as_str) {
        Some("query") => {
            let query = text_at(value, "query").unwrap_or("query");
            let summary = text_at(value, "summary").unwrap_or(query);
            out.push(event(
                "graphify",
                ts,
                "tool_call",
                clip(query, 180),
                json!({ "tool": "query", "input": query }),
                source,
                seq,
            ));
            if !summary.is_empty() && summary != query {
                out.push(event(
                    "graphify",
                    ts,
                    "tool_result",
                    clip(summary, 180),
                    json!({ "output": summary, "detail": value.get("detail") }),
                    source,
                    seq,
                ));
            }
        }
        Some("error") => {
            let summary = text_at(value, "summary")
                .or_else(|| text_at(value, "message"))
                .unwrap_or("error");
            out.push(event(
                "graphify",
                ts,
                "error",
                clip(summary, 180),
                json!({ "text": summary }),
                source,
                seq,
            ));
        }
        Some("status") | None => {
            let summary = text_at(value, "summary")
                .or_else(|| text_at(value, "message"))
                .unwrap_or("status");
            out.push(event(
                "graphify",
                ts,
                "status",
                clip(summary, 180),
                json!({ "text": summary }),
                source,
                seq,
            ));
        }
        Some(_) => {
            let summary = text_at(value, "summary").unwrap_or("update");
            out.push(event(
                "graphify",
                ts,
                "message",
                clip(summary, 180),
                json!({ "text": summary }),
                source,
                seq,
            ));
        }
    }
}

fn parse_orca(value: &Value, ts: &str, source: &str, seq: &mut u64, out: &mut Vec<NormEvent>) {
    let kind = match value.get("type").and_then(Value::as_str) {
        Some("thinking") => "thinking",
        Some("error") => "error",
        Some("status") | Some("delegate") => "status",
        _ => "message",
    };
    let summary = text_at(value, "summary")
        .or_else(|| text_at(value, "text"))
        .or_else(|| text_at(value, "message"))
        .unwrap_or(kind);
    out.push(event(
        "orca",
        ts,
        kind,
        clip(summary, 180),
        json!({
            "text": summary,
            "tasks": value.get("tasks").cloned().unwrap_or(Value::Null),
        }),
        source,
        seq,
    ));
}

fn push_content_block(
    agent: &str,
    block: &Value,
    ts: &str,
    source: &str,
    seq: &mut u64,
    out: &mut Vec<NormEvent>,
) {
    match block.get("type").and_then(Value::as_str) {
        Some("thinking") | Some("redacted_thinking") => {
            let text = text_at(block, "thinking").unwrap_or("");
            if !text.is_empty() {
                out.push(event(
                    agent,
                    ts,
                    "thinking",
                    clip(text, 180),
                    json!({ "text": text }),
                    source,
                    seq,
                ));
            }
        }
        Some("text") => {
            let text = text_at(block, "text").unwrap_or("");
            if !text.is_empty() {
                out.push(event(
                    agent,
                    ts,
                    "message",
                    clip(text, 180),
                    json!({ "text": text }),
                    source,
                    seq,
                ));
            }
        }
        Some("tool_use") => {
            let name = text_at(block, "name").unwrap_or("tool");
            let input = block.get("input").cloned().unwrap_or(Value::Null);
            out.push(event(
                agent,
                ts,
                "tool_call",
                format!("{name} {}", clip(&compact(&input), 140)),
                json!({ "tool": name, "input": input }),
                source,
                seq,
            ));
        }
        Some("tool_result") => {
            let output = content_text(block.get("content"));
            out.push(event(
                agent,
                ts,
                "tool_result",
                clip(&output, 180),
                json!({ "output": output }),
                source,
                seq,
            ));
        }
        _ => {}
    }
}

fn cursor_summary(tool: &str, input: &Value, output: &str) -> String {
    if let Some(command) = input.get("command").and_then(Value::as_str) {
        if !output.is_empty() && !output.starts_with('{') && output.chars().count() < 48 {
            return format!("{command}: {output}");
        }
        return command.to_string();
    }
    if output.is_empty() || output.starts_with('{') {
        tool.to_string()
    } else {
        format!("{tool}: {}", clip(output, 80))
    }
}

fn cursor_tool(tool_call: Option<&Value>) -> (String, Value, String) {
    let Some(tool_call) = tool_call else {
        return ("tool".to_string(), Value::Null, String::new());
    };
    if let Some(name) = tool_call.get("name").and_then(Value::as_str) {
        let input = tool_call.get("input").cloned().unwrap_or(Value::Null);
        let output = content_text(tool_call.get("output").or_else(|| tool_call.get("result")));
        return (name.to_string(), input, output);
    }
    if let Some(object) = tool_call.as_object() {
        if let Some((key, value)) = object.iter().next() {
            let tool = key.trim_end_matches("ToolCall").to_string();
            let input = value.get("args").cloned().unwrap_or(Value::Null);
            let output = content_text(value.get("result"));
            return (tool, input, output);
        }
    }
    ("tool".to_string(), tool_call.clone(), String::new())
}

fn content_text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| text_at(item, "text").map(str::to_string))
            .collect::<Vec<_>>()
            .join("\n"),
        other => compact(other),
    }
}

fn command_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        Some(Value::String(text)) => text.clone(),
        _ => "command".to_string(),
    }
}

fn timestamp_of(value: &Value, fallback: &str) -> String {
    for key in ["timestamp", "ts", "timestamp_ms", "time_ms"] {
        if let Some(found) = value.get(key) {
            return parse_timestamp(found, fallback);
        }
    }
    fallback.to_string()
}

fn text_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut end = max;
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &flat[..end])
}

fn event(
    agent: &str,
    ts: &str,
    kind: &str,
    summary: String,
    detail: Value,
    source: &str,
    seq: &mut u64,
) -> NormEvent {
    let id = *seq;
    *seq += 1;
    NormEvent {
        seq: id,
        ts: ts.to_string(),
        agent: agent.to_string(),
        kind: kind.to_string(),
        summary,
        detail,
        source: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(agent: &str, line: &str) -> Vec<String> {
        let mut seq = 1;
        parse_line(agent, line, "fixture.jsonl", &mut seq)
            .into_iter()
            .map(|event| format!("{}:{}", event.kind, event.summary))
            .collect()
    }

    #[test]
    fn claude_stream_json_splits_thinking_text_and_tools() {
        let line = r#"{"type":"assistant","timestamp":"2026-11-17T14:31:12Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"The ledger is the source of truth."},{"type":"text","text":"I'll read the schema."},{"type":"tool_use","name":"Read","input":{"file_path":"schema.rs"}}]}}"#;
        let events = kinds("claude", line);
        assert_eq!(events[0], "thinking:The ledger is the source of truth.");
        assert_eq!(events[1], "message:I'll read the schema.");
        assert!(events[2].starts_with("tool_call:Read"));
        let mut seq = 1;
        let parsed = parse_line("claude", line, "claude.jsonl", &mut seq);
        assert_eq!(parsed[0].ts, "2026-11-17T14:31:12Z");
        let result = kinds(
            "claude",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"SCHEMA_VERSION = 6"}]}}"#,
        );
        assert_eq!(result, ["tool_result:SCHEMA_VERSION = 6"]);
        let done = kinds(
            "claude",
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Plan is ready."}"#,
        );
        assert_eq!(done, ["status:Plan is ready."]);
        let failed = kinds(
            "claude",
            r#"{"type":"result","subtype":"error","is_error":true,"result":"API error"}"#,
        );
        assert_eq!(failed, ["error:API error"]);
        assert!(kinds(
            "claude",
            r#"{"type":"stream_event","event":{"type":"content_block_delta"}}"#
        )
        .is_empty());
    }

    #[test]
    fn cursor_stream_json_maps_tool_calls() {
        let started = kinds(
            "cursor",
            r#"{"type":"tool_call","subtype":"started","tool_call":{"editToolCall":{"args":{"path":"src/ledger/v6.rs"}}}}"#,
        );
        assert_eq!(started, ["tool_call:edit"]);
        let finished = kinds(
            "cursor",
            r#"{"type":"tool_call","subtype":"completed","tool_call":{"editToolCall":{"args":{"path":"src/ledger/v6.rs"},"result":{"success":true}}}}"#,
        );
        assert!(finished[0].starts_with("tool_result:edit"));
        let text = kinds(
            "cursor",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Implementing the adapter."}]}}"#,
        );
        assert_eq!(text, ["message:Implementing the adapter."]);
        let failed = kinds(
            "cursor",
            r#"{"type":"result","subtype":"error","is_error":true,"result":"build failed"}"#,
        );
        assert_eq!(failed, ["error:build failed"]);
    }

    #[test]
    fn codex_json_and_legacy_messages() {
        let thinking = kinds(
            "codex",
            r#"{"type":"item.completed","item":{"type":"reasoning","text":"Checking the adapter does not write."}}"#,
        );
        assert_eq!(thinking, ["thinking:Checking the adapter does not write."]);
        let message = kinds(
            "codex",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Review looks clean."}}"#,
        );
        assert_eq!(message, ["message:Review looks clean."]);
        let call = kinds(
            "codex",
            r#"{"type":"item.started","item":{"type":"command_execution","command":"cargo test","status":"in_progress"}}"#,
        );
        assert_eq!(call, ["tool_call:cargo test"]);
        let result = kinds(
            "codex",
            r#"{"type":"item.completed","item":{"type":"command_execution","command":"cargo test","aggregated_output":"ok","exit_code":0,"status":"completed"}}"#,
        );
        assert_eq!(result, ["tool_result:ok"]);
        let legacy = kinds(
            "codex",
            r#"{"id":"1","msg":{"type":"agent_message","message":"hello from legacy"}}"#,
        );
        assert_eq!(legacy, ["message:hello from legacy"]);
        let error = kinds("codex", r#"{"type":"error","message":"turn failed"}"#);
        assert_eq!(error, ["error:turn failed"]);
    }

    #[test]
    fn graphify_and_partial_lines() {
        let query = kinds(
            "graphify",
            r#"{"type":"query","query":"schema adapter","summary":"schema.rs owns SCHEMA_VERSION"}"#,
        );
        assert_eq!(query.len(), 2);
        assert!(query[0].starts_with("tool_call:"));
        assert!(query[1].starts_with("tool_result:schema.rs"));
        let plain = kinds("graphify", "index refreshed");
        assert_eq!(plain, ["message:index refreshed"]);
        assert!(kinds("claude", "   ").is_empty());
    }
}
