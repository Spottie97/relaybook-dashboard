use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::model::NormEvent;
use crate::timefmt::{format_unix, now_rfc3339};

use super::adapt::{adapt, classify, is_companion, Role, Sides};
use super::rules::Rule;

const JSON_CAP: u64 = 2 * 1024 * 1024;
const TEXT_CAP: u64 = 256 * 1024;

#[derive(Clone, PartialEq, Eq)]
struct Version {
    len: u64,
    mtime: u128,
    companions: u64,
}

struct Seen {
    version: Version,
    fingerprint: u64,
    pending: bool,
    agent: String,
}

pub struct LogWatch {
    pub rules: Vec<Rule>,
    seen: HashMap<PathBuf, Seen>,
    pub unmapped: usize,
    seq: u64,
}

impl LogWatch {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            seen: HashMap::new(),
            unmapped: 0,
            seq: 1,
        }
    }

    pub fn known_agents(&self) -> Vec<String> {
        let mut names: Vec<String> = self.seen.values().map(|seen| seen.agent.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn poll(&mut self, root: Option<&Path>) -> Vec<NormEvent> {
        let Some(root) = root else {
            self.unmapped = 0;
            return Vec::new();
        };
        if !root.is_dir() {
            self.unmapped = 0;
            return Vec::new();
        }
        let files = list_files(root);
        let mut mapped = Vec::new();
        self.unmapped = 0;
        for path in files {
            let Some(name) = file_name(&path) else {
                continue;
            };
            if name.starts_with('.') || is_companion(&name) {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let role = classify(&name);
            let rule = self
                .rules
                .iter()
                .find(|rule| rule.matcher.is_match(relative) || rule.matcher.is_match(&path));
            match (rule, role) {
                (None, _) => self.unmapped += 1,
                (Some(rule), role) => {
                    let role = role.unwrap_or_else(|| fallback_role(&name));
                    mapped.push((path, rule.agent.clone(), role));
                }
            }
        }
        self.seen
            .retain(|path, _| mapped.iter().any(|(item, _, _)| item == path));
        let mut drafts = Vec::new();
        for (path, agent, role) in mapped {
            if let Some(draft) = self.read_file(&path, &agent, role) {
                drafts.push(draft);
            }
        }
        drafts.sort_by(|left, right| left.ts.cmp(&right.ts).then(left.source.cmp(&right.source)));
        for event in &mut drafts {
            event.seq = self.seq;
            self.seq += 1;
        }
        drafts
    }

    fn read_file(&mut self, path: &Path, agent: &str, role: Role) -> Option<NormEvent> {
        let meta = fs::metadata(path).ok()?;
        let name = file_name(path)?;
        let version = Version {
            len: meta.len(),
            mtime: mtime_nanos(&meta),
            companions: companion_token(path, &name),
        };
        if let Some(seen) = self.seen.get(path) {
            if seen.version == version && !seen.pending {
                return None;
            }
        }
        let body = read_for(role, path)?;
        let mtime = mtime_rfc3339(&meta);
        let stem = stem_of(&name);
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let meta_text = read_prefix(&dir.join(format!("{stem}.meta")), 64 * 1024);
        let stderr = read_prefix(&dir.join(format!("{stem}.stderr")), 64 * 1024);
        let result_md = read_prefix(&dir.join(format!("{stem}.result.md")), 256 * 1024);
        let sides = Sides {
            meta: meta_text.as_deref(),
            stderr: stderr.as_deref(),
            result_md: result_md.as_deref(),
        };
        let draft = adapt(role, agent, &name, &body, &sides, &mtime);
        let fingerprint = fingerprint(&draft);
        let pending = is_pending(&draft);
        if self
            .seen
            .get(path)
            .is_some_and(|seen| seen.fingerprint == fingerprint)
        {
            if let Some(seen) = self.seen.get_mut(path) {
                seen.version = version;
                seen.pending = pending;
                seen.agent = agent.to_string();
            }
            return None;
        }
        self.seen.insert(
            path.to_path_buf(),
            Seen {
                version,
                fingerprint,
                pending,
                agent: agent.to_string(),
            },
        );
        Some(NormEvent {
            seq: 0,
            ts: draft.ts,
            agent: draft.agent,
            kind: draft.kind,
            summary: draft.summary,
            detail: draft.detail,
            source: name,
        })
    }
}

fn fallback_role(name: &str) -> Role {
    if name.to_ascii_lowercase().ends_with(".json") {
        Role::JsonBlob
    } else {
        Role::TextBlob
    }
}

fn is_pending(draft: &super::adapt::Draft) -> bool {
    matches!(
        draft.detail.get("state").and_then(Value::as_str),
        Some("writing") | Some("parse_error")
    )
}

fn fingerprint(draft: &super::adapt::Draft) -> u64 {
    let mut hasher = DefaultHasher::new();
    draft.kind.hash(&mut hasher);
    draft.summary.hash(&mut hasher);
    draft.ts.hash(&mut hasher);
    draft.agent.hash(&mut hasher);
    draft.detail.to_string().hash(&mut hasher);
    hasher.finish()
}

fn read_for(role: Role, path: &Path) -> Option<String> {
    if matches!(
        role,
        Role::Graphify | Role::Gate | Role::CodexTranscript | Role::TextBlob
    ) {
        read_tail(path, TEXT_CAP)
    } else {
        read_prefix(path, JSON_CAP)
    }
}

fn read_prefix(path: &Path, cap: u64) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let file = File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(cap).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn read_tail(path: &Path, cap: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > cap {
        file.seek(SeekFrom::End(-(cap as i64))).ok()?;
    }
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    if len > cap {
        if let Some(pos) = buf.iter().position(|byte| *byte == b'\n') {
            buf.drain(..=pos);
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn companion_token(path: &Path, name: &str) -> u64 {
    let Some(dir) = path.parent() else {
        return 0;
    };
    let stem = stem_of(name);
    let mut token = 0u64;
    for suffix in [".meta", ".stderr", ".result.md", "-prompt.md"] {
        let candidate = dir.join(format!("{stem}{suffix}"));
        let Ok(meta) = fs::metadata(&candidate) else {
            continue;
        };
        token = token
            .wrapping_mul(31)
            .wrapping_add(meta.len())
            .wrapping_add(mtime_nanos(&meta) as u64);
    }
    token
}

fn stem_of(name: &str) -> &str {
    name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name)
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

fn mtime_nanos(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn mtime_rfc3339(meta: &fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| format_unix(duration.as_secs() as i64))
        .unwrap_or_else(now_rfc3339)
}

fn list_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(root, &mut files, 0);
    files.sort();
    files
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files, depth + 1);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::rules_from_config;

    #[test]
    fn partial_json_retries_and_companions_are_not_events() {
        let dir = tempfile::tempdir().unwrap();
        let json = dir.path().join("claude-rev3-run.json");
        fs::write(&json, "{\"type\":\"result\",\"result\":").unwrap();
        fs::write(dir.path().join("claude-rev3-run.meta"), "exit=0\n").unwrap();
        fs::write(dir.path().join("claude-rev3-run.stderr"), "PY\n").unwrap();
        fs::write(dir.path().join("claude-rev3-run-prompt.md"), "PY\n").unwrap();
        let mut watch = LogWatch::new(rules_from_config(&[]).unwrap());
        let events = watch.poll(Some(dir.path()));
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(
            events[0].summary.contains("writing…"),
            "{}",
            events[0].summary
        );
        assert!(watch.poll(Some(dir.path())).is_empty());
        fs::write(
            &json,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Plan accepted. The ledger stays read-only.","duration_ms":10,"session_id":"s"}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("claude-rev3-run.meta"),
            "exit=0\nend_utc=2026-11-17T14:31:12Z\nelapsed_s=4\n",
        )
        .unwrap();
        let events = watch.poll(Some(dir.path()));
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(events[0].summary.starts_with("rev3 · Plan accepted."));
        assert_ne!(events[0].summary, "PY");
        assert!(!events[0].summary.contains("python"));
        assert_eq!(events[0].ts, "2026-11-17T14:31:12Z");
        assert!(watch.poll(Some(dir.path())).is_empty());
    }

    #[test]
    fn directory_uses_real_names_and_hides_the_codex_transcript() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("m1-cursor-impl.json"),
            r#"{"type":"result","is_error":false,"result":"PY","session_id":"c"}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("m1-cursor-impl.result.md"),
            "Opened the ledger read-only and checked user_version.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("m1-cursor-impl-prompt.md"),
            "PY\nignore this prompt\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("r2-cursor-fix1.json"),
            r#"{"type":"result","is_error":false,"result":"Fixed the torn read."}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("m1-codex-r1.md"),
            "# Verdict: accept\n\nThe copy does not write the source directory.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("codex-m1-run.log"),
            "bash -lc 'cargo test'\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("m1-graphify-update.log"),
            "scanning\n\nindex refreshed\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("m1-cargo-test.log"),
            "test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("orca-orchestrator.jsonl"),
            "{\"summary\":\"nope\"}\n",
        )
        .unwrap();
        let mut watch = LogWatch::new(rules_from_config(&[]).unwrap());
        let events = watch.poll(Some(dir.path()));
        let summary = |agent: &str, kind: &str| {
            events
                .iter()
                .find(|event| event.agent == agent && event.kind == kind)
                .map(|event| event.summary.clone())
        };
        assert!(
            summary("cursor", "status")
                .unwrap()
                .contains("Opened the ledger read-only"),
            "{events:?}"
        );
        assert!(events
            .iter()
            .any(|event| event.summary.contains("Fixed the torn read")));
        assert!(events
            .iter()
            .any(|event| event.kind == "review" && event.summary.contains("Verdict: accept")));
        assert!(events
            .iter()
            .any(|event| event.kind == "transcript" && !event.summary.contains("cargo")));
        assert!(events
            .iter()
            .any(|event| event.summary.contains("index refreshed")));
        assert!(events
            .iter()
            .any(|event| event.kind == "gate" && event.summary.contains("test result: ok")));
        assert!(events.iter().all(|event| event.agent != "orca"));
        assert!(events
            .iter()
            .all(|event| !event.summary.contains("ignore this prompt")));
        assert_eq!(watch.unmapped, 1, "orca file is unmapped");
    }
}
