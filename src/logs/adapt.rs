use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::timefmt::{now_rfc3339, parse_timestamp};

/// What a context file is, based on its name. Sidecars are not roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    ClaudeJson,
    CursorJson,
    CodexReview,
    CodexTranscript,
    Graphify,
    Gate,
    /// A custom log-map hit that is one JSON document.
    JsonBlob,
    /// A custom log-map hit that is plain text.
    TextBlob,
}

pub struct Sides<'a> {
    pub meta: Option<&'a str>,
    pub stderr: Option<&'a str>,
    pub result_md: Option<&'a str>,
}

impl<'a> Sides<'a> {
    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            meta: None,
            stderr: None,
            result_md: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub ts: String,
    pub agent: String,
    pub kind: String,
    pub summary: String,
    pub detail: Value,
}

pub fn classify(name: &str) -> Option<Role> {
    if is_companion(name) || name.starts_with('.') {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("claude-") && lower.ends_with("-run.json") {
        return Some(Role::ClaudeJson);
    }
    if cursor_phase(&lower).is_some() {
        return Some(Role::CursorJson);
    }
    if lower.contains("-codex-r") && (lower.ends_with(".md") || lower.ends_with(".markdown")) {
        return Some(Role::CodexReview);
    }
    if lower.starts_with("codex-") && lower.ends_with("-run.log") {
        return Some(Role::CodexTranscript);
    }
    if lower.contains("-graphify-update.log") || lower.contains("-graphify-postcommit.log") {
        return Some(Role::Graphify);
    }
    if lower.ends_with(".log")
        && !lower.starts_with("codex-")
        && !lower.contains("graphify")
        && (lower.contains("-verify") || lower.contains("-cargo-test"))
    {
        return Some(Role::Gate);
    }
    None
}

pub fn is_companion(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".meta")
        || lower.ends_with(".stderr")
        || lower.ends_with(".result.md")
        || lower.ends_with("-prompt.md")
}

/// Milestone token from a flat context filename.
/// `claude-rev3-run.json` → `rev3`, `r2-cursor-fix1.json` → `r2`.
pub fn milestone(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("claude-") && lower.ends_with("-run.json") && name.len() == lower.len() {
        let end = name.len() - "-run.json".len();
        return name["claude-".len()..end].to_string();
    }
    if lower.starts_with("codex-") && lower.ends_with("-run.log") && name.len() == lower.len() {
        let end = name.len() - "-run.log".len();
        return name["codex-".len()..end].to_string();
    }
    for marker in [
        "-cursor-",
        "-codex-",
        "-graphify-",
        "-cargo-test",
        "-verify",
    ] {
        if let Some(index) = lower.find(marker) {
            if index > 0 && name.is_char_boundary(index) {
                return name[..index].to_string();
            }
        }
    }
    String::new()
}

pub fn adapt(
    role: Role,
    agent: &str,
    name: &str,
    body: &str,
    sides: &Sides<'_>,
    mtime: &str,
) -> Draft {
    let meta = sides.meta.map(parse_meta).unwrap_or_default();
    let milestone = milestone(name);
    let mut draft = match role {
        Role::ClaudeJson | Role::JsonBlob => {
            adapt_result(agent, name, body, &meta, None, &milestone, mtime, false)
        }
        Role::CursorJson => adapt_result(
            agent,
            name,
            body,
            &meta,
            sides.result_md,
            &milestone,
            mtime,
            true,
        ),
        Role::CodexReview => adapt_review(agent, name, body, &meta, &milestone, mtime),
        Role::CodexTranscript => adapt_transcript(agent, name, body, &meta, &milestone, mtime),
        Role::Graphify | Role::TextBlob => {
            adapt_last_line(agent, name, body, &meta, &milestone, mtime, role)
        }
        Role::Gate => adapt_gate(agent, name, body, &meta, &milestone, mtime),
    };
    attach_stderr(&mut draft, sides.stderr);
    draft
}

fn adapt_result(
    agent: &str,
    name: &str,
    body: &str,
    meta: &BTreeMap<String, String>,
    result_md: Option<&str>,
    milestone: &str,
    mtime: &str,
    prefer_md: bool,
) -> Draft {
    let parsed = parse_json_document(body);
    let ts = choose_ts(meta, parsed.as_ref().ok(), mtime);
    let failed = parsed
        .as_ref()
        .ok()
        .is_some_and(|value| run_failed(value, meta));
    let (kind, summary, state) = match parsed.as_ref() {
        Err(problem) => (
            if *problem == "writing…" {
                "status"
            } else {
                "error"
            },
            labeled(milestone, problem),
            if *problem == "writing…" {
                "writing"
            } else {
                "parse_error"
            },
        ),
        Ok(value) => {
            let from_md = if prefer_md {
                result_md.and_then(first_readable)
            } else {
                None
            };
            let result = result_text(value);
            let summary_body = if let Some(text) = from_md {
                text
            } else if !result.is_empty() {
                result
            } else if failed {
                "run failed".to_string()
            } else {
                "run finished".to_string()
            };
            (
                if failed { "error" } else { "status" },
                labeled(milestone, &summary_body),
                "ok",
            )
        }
    };
    let mut detail = json!({
        "milestone": milestone,
        "file": name,
        "state": state,
    });
    if let Ok(value) = parsed.as_ref() {
        let object = detail.as_object_mut().expect("detail object");
        object.insert(
            "text".to_string(),
            Value::String(shown_text(result_md, value)),
        );
        if let Some(subtype) = text_at(value, "subtype") {
            object.insert("subtype".to_string(), json!(subtype));
        }
        object.insert("is_error".to_string(), json!(failed));
        if let Some(session) = text_at(value, "session_id").or_else(|| text_at(value, "sessionId"))
        {
            object.insert("session_id".to_string(), json!(session));
        }
        if let Some(duration) = value.get("duration_ms").or_else(|| value.get("duration")) {
            object.insert("duration_ms".to_string(), duration.clone());
        }
        if let Some(turns) = value.get("num_turns") {
            object.insert("num_turns".to_string(), turns.clone());
        }
        if let Some(cost) = value.get("total_cost_usd") {
            object.insert("total_cost_usd".to_string(), cost.clone());
        }
        if let Some(usage) = value.get("usage") {
            object.insert("usage".to_string(), usage.clone());
        }
        if prefer_md && result_md.and_then(first_readable).is_some() {
            object.insert("summary_from".to_string(), json!("result.md"));
        }
    }
    Draft {
        ts,
        agent: agent.to_string(),
        kind: kind.to_string(),
        summary,
        detail: finish_detail(detail, meta),
    }
}

fn shown_text(result_md: Option<&str>, value: &Value) -> String {
    if let Some(markdown) = result_md.map(str::trim).filter(|text| !text.is_empty()) {
        return clip_keep_breaks(markdown, 8000);
    }
    clip_keep_breaks(&result_text(value), 8000)
}

fn finish_detail(mut detail: Value, meta: &BTreeMap<String, String>) -> Value {
    if let Some(object) = detail.as_object_mut() {
        if let Some(exit) = meta.get("exit") {
            object.insert("exit".to_string(), json!(exit));
        }
        if let Some(elapsed) = meta.get("elapsed_s") {
            object.insert("elapsed_s".to_string(), json!(elapsed));
        }
    }
    detail
}

fn adapt_review(
    agent: &str,
    name: &str,
    body: &str,
    meta: &BTreeMap<String, String>,
    milestone: &str,
    mtime: &str,
) -> Draft {
    let summary_body = review_summary(body).unwrap_or_else(|| "review".to_string());
    let detail = json!({
        "milestone": milestone,
        "file": name,
        "state": "ok",
        "text": clip_keep_breaks(body.trim(), 8000),
    });
    Draft {
        ts: choose_ts(meta, None, mtime),
        agent: agent.to_string(),
        kind: "review".to_string(),
        summary: labeled(milestone, &summary_body),
        detail: finish_detail(detail, meta),
    }
}

fn adapt_transcript(
    agent: &str,
    name: &str,
    body: &str,
    meta: &BTreeMap<String, String>,
    milestone: &str,
    mtime: &str,
) -> Draft {
    let detail = json!({
        "milestone": milestone,
        "file": name,
        "state": "ok",
        "text": clip_keep_breaks(body.trim(), 8000),
    });
    Draft {
        ts: choose_ts(meta, None, mtime),
        agent: agent.to_string(),
        kind: "transcript".to_string(),
        summary: labeled(milestone, "run transcript"),
        detail: finish_detail(detail, meta),
    }
}

fn adapt_last_line(
    agent: &str,
    name: &str,
    body: &str,
    meta: &BTreeMap<String, String>,
    milestone: &str,
    mtime: &str,
    role: Role,
) -> Draft {
    let line = last_meaningful(body);
    let summary_body = if line.is_empty() {
        "writing…"
    } else {
        line.as_str()
    };
    let state = if line.is_empty() { "writing" } else { "ok" };
    let detail = json!({
        "milestone": milestone,
        "file": name,
        "state": state,
        "text": clip_keep_breaks(body.trim(), 4000),
    });
    let kind = if role == Role::Graphify {
        "status"
    } else {
        "message"
    };
    Draft {
        ts: choose_ts(meta, None, mtime),
        agent: agent.to_string(),
        kind: kind.to_string(),
        summary: labeled(milestone, summary_body),
        detail: finish_detail(detail, meta),
    }
}

fn adapt_gate(
    agent: &str,
    name: &str,
    body: &str,
    meta: &BTreeMap<String, String>,
    milestone: &str,
    mtime: &str,
) -> Draft {
    let gate = if name.to_ascii_lowercase().contains("cargo-test") {
        "cargo-test"
    } else {
        "verify"
    };
    let exit = meta.get("exit").cloned().or_else(|| exit_in_text(body));
    let result_line = last_line_containing(body, "test result:");
    let (ok, summary_body) = if let Some(line) = result_line.clone() {
        let mut passed = cargo_passed(&line);
        if exit.as_deref().is_some_and(|code| code != "0") {
            passed = false;
        }
        let text = if passed {
            line
        } else if line.to_ascii_lowercase().contains("fail") {
            line
        } else {
            format!("failed · {line}")
        };
        (passed, text)
    } else if let Some(code) = exit.clone() {
        let passed = code == "0";
        let word = if passed { "passed" } else { "failed" };
        (passed, format!("{gate} {word} (exit={code})"))
    } else {
        let line = last_meaningful(body);
        if line.is_empty() {
            (false, "writing…".to_string())
        } else {
            let passed = !looks_failed(&line);
            (passed, line)
        }
    };
    let state = if summary_body == "writing…" {
        "writing"
    } else {
        "ok"
    };
    let mut detail = json!({
        "milestone": milestone,
        "file": name,
        "state": state,
        "gate": gate,
        "ok": ok && state == "ok",
        "text": clip_keep_breaks(body.trim(), 4000),
    });
    if let Some(code) = &exit {
        detail
            .as_object_mut()
            .expect("detail object")
            .insert("exit".to_string(), json!(code));
    }
    let kind = if state == "writing" {
        "status"
    } else if ok {
        "gate"
    } else {
        "error"
    };
    Draft {
        ts: choose_ts(meta, None, mtime),
        agent: agent.to_string(),
        kind: kind.to_string(),
        summary: labeled(milestone, &summary_body),
        detail: finish_detail(detail, meta),
    }
}

fn cargo_passed(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("test result: fail") {
        return false;
    }
    lower.contains("test result: ok")
}

fn looks_failed(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("fail") || lower.contains("error") || lower.contains("panic")
}

fn exit_in_text(text: &str) -> Option<String> {
    for line in text.lines().rev() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("exit=") {
            let raw = trimmed[trimmed.len() - rest.len()..].trim();
            let token = raw.split_whitespace().next().unwrap_or(raw);
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn last_line_containing(text: &str, needle: &str) -> Option<String> {
    let needle = needle.to_ascii_lowercase();
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| line.to_ascii_lowercase().contains(&needle) && meaningful(line))
        .map(str::to_string)
}

pub fn parse_meta(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"').trim().to_string();
        if !key.is_empty() && !value.is_empty() {
            map.insert(key, value);
        }
    }
    map
}

fn choose_ts(meta: &BTreeMap<String, String>, json: Option<&Value>, mtime: &str) -> String {
    if let Some(end) = meta.get("end_utc").and_then(|text| normalize_clock(text)) {
        return end;
    }
    if let Some(value) = json.and_then(json_timestamp) {
        return value;
    }
    if !mtime.is_empty() {
        return mtime.to_string();
    }
    now_rfc3339()
}

fn json_timestamp(value: &Value) -> Option<String> {
    for key in ["timestamp", "ts", "end_utc", "completed_at"] {
        let Some(found) = value.get(key) else {
            continue;
        };
        if let Some(text) = found.as_str() {
            if let Some(clock) = normalize_clock(text) {
                return Some(clock);
            }
        }
        if found.is_number() {
            return Some(parse_timestamp(found, &now_rfc3339()));
        }
    }
    None
}

fn normalize_clock(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_matches('"');
    if trimmed.len() >= 19 && trimmed.as_bytes().get(4) == Some(&b'-') && trimmed.contains('T') {
        return Some(trimmed.to_string());
    }
    None
}

fn parse_json_document(body: &str) -> Result<Value, &'static str> {
    let trimmed = body.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return Err("writing…");
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) if value.is_object() => Ok(value),
        Ok(_) => Err("parse error"),
        Err(_) if looks_partial(trimmed) => Err("writing…"),
        Err(_) => Err("parse error"),
    }
}

fn looks_partial(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return false;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for ch in trimmed.chars() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    in_string || escape || depth != 0
}

fn run_failed(value: &Value, meta: &BTreeMap<String, String>) -> bool {
    if value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    if value.get("subtype").and_then(Value::as_str) == Some("error") {
        return true;
    }
    meta.get("exit").is_some_and(|code| code != "0")
}

fn result_text(value: &Value) -> String {
    match value.get("result") {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn review_summary(markdown: &str) -> Option<String> {
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(heading) = heading_text(trimmed) {
            if !heading.is_empty() {
                return Some(heading);
            }
            continue;
        }
        if is_verdict_line(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    first_readable(markdown)
}

fn is_verdict_line(line: &str) -> bool {
    let bare = heading_text(line).unwrap_or_else(|| line.to_string());
    let lower = bare.to_ascii_lowercase();
    lower.starts_with("verdict")
        || lower.starts_with("decision")
        || lower.starts_with("result:")
        || lower.starts_with("accept")
        || lower.starts_with("reject")
        || lower == "lgtm"
        || lower.starts_with("lgtm ")
}

fn first_readable(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(heading) = heading_text(trimmed) {
            if !heading.is_empty() {
                return Some(heading);
            }
            continue;
        }
        return Some(trimmed.to_string());
    }
    None
}

fn heading_text(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    Some(trimmed.trim_start_matches('#').trim().to_string())
}

fn last_meaningful(text: &str) -> String {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| meaningful(line))
        .unwrap_or("")
        .to_string()
}

fn meaningful(line: &str) -> bool {
    let bare: String = line
        .chars()
        .filter(|ch| !matches!(ch, '-' | '=' | '#' | '*' | '_' | '·' | ' '))
        .collect();
    !bare.is_empty()
}

fn cursor_phase(lower_name: &str) -> Option<&str> {
    if !lower_name.ends_with(".json") {
        return None;
    }
    let rest = lower_name.split("-cursor-").nth(1)?;
    let phase = rest.trim_end_matches(".json");
    if phase == "impl" || phase.starts_with("fix") {
        Some(phase)
    } else {
        None
    }
}

fn labeled(milestone: &str, text: &str) -> String {
    let clipped = clip(text, 160);
    if milestone.is_empty()
        || clipped
            .to_ascii_lowercase()
            .starts_with(&milestone.to_ascii_lowercase())
    {
        clipped
    } else {
        clip(&format!("{milestone} · {clipped}"), 180)
    }
}

fn text_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

pub fn clip(text: &str, max: usize) -> String {
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

fn clip_keep_breaks(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .nth(max)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}…", &text[..end])
}

/// Attach stderr without letting it become the summary. Called by the watcher
/// after adapt, so a sidecar full of shell or a language tag stays in detail.
pub fn attach_stderr(draft: &mut Draft, stderr: Option<&str>) {
    let Some(text) = stderr.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    let Some(object) = draft.detail.as_object_mut() else {
        return;
    };
    object.insert("stderr".to_string(), Value::String(clip(text, 2000)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(role: Role, name: &str, body: &str, sides: Sides<'_>) -> Draft {
        adapt(
            role,
            agent_of(role),
            name,
            body,
            &sides,
            "2026-11-17T12:00:00Z",
        )
    }

    fn agent_of(role: Role) -> &'static str {
        match role {
            Role::ClaudeJson => "claude",
            Role::CursorJson => "cursor",
            Role::CodexReview | Role::CodexTranscript => "codex",
            Role::Graphify => "graphify",
            Role::Gate => "cursor",
            Role::JsonBlob => "claude",
            Role::TextBlob => "graphify",
        }
    }

    #[test]
    fn claude_result_uses_the_result_text_and_meta_time() {
        let body = r#"{
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "Plan accepted. The ledger stays the source of truth, and the dashboard shows this sentence instead of guessing a language from a later code fence. The reader copies the database aside.\n\n```python\nprint('hi')\n```",
            "duration_ms": 18200,
            "num_turns": 4,
            "total_cost_usd": 0.18,
            "session_id": "demo-claude",
            "usage": {"input_tokens": 12, "output_tokens": 4}
        }"#;
        let meta = "exit=0\nend_utc=2026-11-17T14:31:12Z\nelapsed_s=18\n";
        let event = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            body,
            Sides {
                meta: Some(meta),
                stderr: Some("PY\n"),
                result_md: None,
            },
        );
        assert_eq!(event.agent, "claude");
        assert_eq!(event.kind, "status");
        assert_eq!(event.ts, "2026-11-17T14:31:12Z");
        assert!(event.summary.starts_with("rev3 · Plan accepted."));
        assert!(!event.summary.contains("print"));
        assert!(!event.summary.contains("```"));
        assert_ne!(event.summary, "PY");
        assert_ne!(event.summary, "rev3 · PY");
        assert_ne!(event.summary, "python");
        assert_eq!(event.detail["milestone"], "rev3");
        assert_eq!(event.detail["session_id"], "demo-claude");
        assert_eq!(event.detail["exit"], "0");
        let failed = draft(
            Role::ClaudeJson,
            "claude-r1-run.json",
            r#"{"type":"result","subtype":"error","is_error":true,"result":"API error"}"#,
            Sides::none(),
        );
        assert_eq!(failed.kind, "error");
        assert!(failed.summary.contains("API error"));
        assert_eq!(failed.detail["milestone"], "r1");
    }

    #[test]
    fn claude_does_not_read_stream_content_blocks() {
        let body = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"PY"},{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}]}}"#;
        let event = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            body,
            Sides::none(),
        );
        assert_eq!(event.summary, "rev3 · run finished");
        assert!(!event.summary.contains("PY"));
        assert!(!event.summary.contains("cargo"));
    }

    #[test]
    fn partial_and_malformed_json_do_not_panic() {
        let partial = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            "{\"type\":\"result\",\"result\":",
            Sides::none(),
        );
        assert!(partial.summary.contains("writing…"));
        assert_eq!(partial.detail["state"], "writing");
        assert_eq!(partial.kind, "status");
        let empty = draft(
            Role::CursorJson,
            "m1-cursor-impl.json",
            "   ",
            Sides::none(),
        );
        assert!(empty.summary.contains("writing…"));
        let broken = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            "{\"type\":",
            Sides::none(),
        );
        assert!(broken.summary.contains("writing…"), "{}", broken.summary);
        let garbage = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            "this is not json",
            Sides::none(),
        );
        assert!(garbage.summary.contains("parse error"));
        assert_eq!(garbage.kind, "error");
        assert_eq!(garbage.detail["state"], "parse_error");
    }

    #[test]
    fn cursor_prefers_result_md_and_accepts_r_milestone_names() {
        let json = r#"{"type":"result","subtype":"success","is_error":false,"result":"PY","duration_ms":900,"session_id":"cur-1"}"#;
        let markdown = "Opened the ledger read-only and checked user_version.\n";
        let event = draft(
            Role::CursorJson,
            "r2-cursor-fix1.json",
            json,
            Sides {
                meta: Some("exit=0\nend_utc=2026-11-17T15:00:00Z\nelapsed_s=9\n"),
                stderr: None,
                result_md: Some(markdown),
            },
        );
        assert_eq!(event.detail["milestone"], "r2");
        assert!(event.summary.contains("Opened the ledger read-only"));
        assert!(!event.summary.contains("PY"));
        assert_eq!(event.ts, "2026-11-17T15:00:00Z");
        assert_eq!(event.detail["summary_from"], "result.md");
        let impl_file = draft(
            Role::CursorJson,
            "m1-cursor-impl.json",
            r#"{"type":"result","is_error":false,"result":"Implemented the adapter."}"#,
            Sides::none(),
        );
        assert!(impl_file.summary.contains("Implemented the adapter."));
        assert_eq!(impl_file.detail["milestone"], "m1");
        assert!(classify("Rev3-cursor-impl.json") == Some(Role::CursorJson));
        assert_eq!(milestone("Rev3-cursor-fix12.json"), "Rev3");
    }

    #[test]
    fn json_timestamp_is_used_when_meta_has_no_end() {
        let event = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            r#"{"type":"result","is_error":false,"result":"Done.","timestamp":"2026-11-17T16:02:03Z"}"#,
            Sides {
                meta: Some("exit=0\nelapsed_s=3\n"),
                stderr: None,
                result_md: None,
            },
        );
        assert_eq!(event.ts, "2026-11-17T16:02:03Z");
        let from_mtime = draft(
            Role::ClaudeJson,
            "claude-rev3-run.json",
            r#"{"type":"result","is_error":false,"result":"Done."}"#,
            Sides::none(),
        );
        assert_eq!(from_mtime.ts, "2026-11-17T12:00:00Z");
    }

    #[test]
    fn codex_review_summary_is_the_heading_or_verdict_not_the_run_log() {
        let review = "# Verdict: accept\n\nThe copy does not write the source directory.\n";
        let event = draft(Role::CodexReview, "m1-codex-r1.md", review, Sides::none());
        assert_eq!(event.kind, "review");
        assert_eq!(event.summary, "m1 · Verdict: accept");
        assert!(event.detail["text"]
            .as_str()
            .unwrap()
            .contains("does not write"));
        let verdict_later = draft(
            Role::CodexReview,
            "rev3-codex-review.md",
            "Verdict: reject\n\n# Notes\n\nThe diff writes the ledger.\n",
            Sides::none(),
        );
        assert_eq!(verdict_later.detail["milestone"], "rev3");
        assert!(verdict_later.summary.contains("Verdict: reject"));
        let transcript = draft(
            Role::CodexTranscript,
            "codex-m1-run.log",
            "exec\nbash -lc 'cargo test'\n# reading diff\n",
            Sides::none(),
        );
        assert_eq!(transcript.kind, "transcript");
        assert_eq!(transcript.summary, "m1 · run transcript");
        assert!(!transcript.summary.contains("cargo"));
        assert!(transcript.detail["text"]
            .as_str()
            .unwrap()
            .contains("cargo test"));
    }

    #[test]
    fn graphify_keeps_the_last_meaningful_line() {
        let body = "scanning\n\n---\n\nschema.rs owns SCHEMA_VERSION\n\n";
        let event = draft(
            Role::Graphify,
            "m1-graphify-update.log",
            body,
            Sides::none(),
        );
        assert_eq!(event.summary, "m1 · schema.rs owns SCHEMA_VERSION");
        let post = draft(
            Role::Graphify,
            "r1-graphify-postcommit.log",
            "\n===\npost-commit index updated\n",
            Sides::none(),
        );
        assert_eq!(post.detail["milestone"], "r1");
        assert!(post.summary.contains("post-commit index updated"));
    }

    #[test]
    fn gate_logs_use_the_cargo_line_or_meta_exit() {
        let passed = draft(
            Role::Gate,
            "m1-cargo-test.log",
            "running 4 tests\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            Sides {
                meta: Some("exit=0\nend_utc=2026-11-17T15:10:00Z\n"),
                stderr: None,
                result_md: None,
            },
        );
        assert_eq!(passed.kind, "gate");
        assert!(passed.summary.contains("test result: ok"));
        assert_eq!(passed.detail["ok"], true);
        assert_eq!(passed.ts, "2026-11-17T15:10:00Z");
        let failed = draft(
            Role::Gate,
            "r2-cargo-test.log",
            "test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out\n",
            Sides::none(),
        );
        assert_eq!(failed.kind, "error");
        assert_eq!(failed.detail["ok"], false);
        assert_eq!(failed.detail["milestone"], "r2");
        let verify = draft(
            Role::Gate,
            "m1-verify-fmt.log",
            "checking fmt\n",
            Sides {
                meta: Some("exit=1\n"),
                stderr: None,
                result_md: None,
            },
        );
        assert!(verify.summary.contains("verify failed"));
        assert!(verify.summary.contains("exit=1"));
        assert_eq!(verify.kind, "error");
        let verify_ok = draft(
            Role::Gate,
            "rev3-verify.log",
            "fmt check ok\n",
            Sides {
                meta: Some("exit=0\n"),
                stderr: None,
                result_md: None,
            },
        );
        assert!(verify_ok.summary.contains("verify passed"));
        assert_eq!(verify_ok.kind, "gate");
    }

    #[test]
    fn companions_and_filenames_classify_without_orca() {
        assert!(classify("claude-rev3-run.meta").is_none());
        assert!(classify("claude-rev3-run.stderr").is_none());
        assert!(classify("m1-cursor-impl.result.md").is_none());
        assert!(classify("m1-cursor-impl-prompt.md").is_none());
        assert!(classify("orca-orchestrator.jsonl").is_none());
        assert_eq!(classify("claude-rev3-run.json"), Some(Role::ClaudeJson));
        assert_eq!(classify("codex-rev3-run.log"), Some(Role::CodexTranscript));
        assert_eq!(classify("m1-codex-r2.md"), Some(Role::CodexReview));
        assert_eq!(classify("m1-verify.log"), Some(Role::Gate));
    }
}
