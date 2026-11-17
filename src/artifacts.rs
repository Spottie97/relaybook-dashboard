use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ledger::{DecisionRow, ReportRow};
use crate::model::{ArtifactBody, ArtifactRef};
use crate::timefmt::format_unix;

const MAX_BYTES: u64 = 256 * 1024;

pub fn list_files(root: &Path) -> Vec<ArtifactRef> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut found = Vec::new();
    walk(root, root, &mut found, 0);
    found.sort_by(|left, right| right.modified.cmp(&left.modified));
    found.truncate(24);
    found
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<ArtifactRef>, depth: usize) {
    if depth > 2 || found.len() >= 24 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= 24 {
            return;
        }
        if path.is_dir() {
            walk(root, &path, found, depth + 1);
            continue;
        }
        if !wanted(&path) {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let rel = relative.to_string_lossy().replace('\\', "/");
        found.push(ArtifactRef {
            id: format!("file:{rel}"),
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or(rel.clone()),
            kind: kind_for(&path).to_string(),
            detail: format!("{} · {}", human_bytes(meta.len()), modified(&meta)),
            bytes: meta.len(),
            modified: modified(&meta),
        });
    }
}

pub fn load_report(report: &ReportRow, slug: &str) -> ArtifactBody {
    let pretty = pretty_body(&report.body);
    ArtifactBody {
        id: format!("report:{}", report.id),
        name: format!("{slug}.report.json"),
        kind: "report".to_string(),
        language: "json".to_string(),
        content: pretty,
        truncated: false,
        bytes: report.bytes.max(0) as u64,
        modified: report.created_at.clone(),
    }
}

pub fn load_decision(decision: &DecisionRow, slug: &str) -> ArtifactBody {
    let content = format!(
        "# Decision\n\nRecorded by {}.\n\n{}\n",
        decision.provenance, decision.rationale
    );
    ArtifactBody {
        id: format!("decision:{}", decision.id),
        name: format!("{slug}.decision.md"),
        kind: "decision".to_string(),
        language: "markdown".to_string(),
        content,
        truncated: false,
        bytes: decision.rationale.len() as u64,
        modified: decision.created_at.clone(),
    }
}

pub fn load_file(root: &Path, id: &str) -> Option<ArtifactBody> {
    let rel = id.strip_prefix("file:")?;
    let path = safe_join(root, rel)?;
    let meta = fs::metadata(&path).ok()?;
    let mut truncated = false;
    let bytes = fs::read(&path).ok()?;
    let (slice, cut) = if bytes.len() as u64 > MAX_BYTES {
        truncated = true;
        (&bytes[..MAX_BYTES as usize], true)
    } else {
        (bytes.as_slice(), false)
    };
    let _ = cut;
    let content = String::from_utf8_lossy(slice).into_owned();
    Some(ArtifactBody {
        id: id.to_string(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| rel.to_string()),
        kind: kind_for(&path).to_string(),
        language: language_for(&path).to_string(),
        content,
        truncated,
        bytes: meta.len(),
        modified: modified(&meta),
    })
}

fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains("..") || rel.starts_with('/') || rel.contains('\\') {
        return None;
    }
    let root = fs::canonicalize(root).ok()?;
    let candidate = root.join(rel);
    let file = fs::canonicalize(&candidate).ok()?;
    if file.starts_with(&root) && file.is_file() {
        Some(file)
    } else {
        None
    }
}

fn wanted(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name.starts_with('.') || name.ends_with(".jsonl") || name.ends_with(".db") {
        return false;
    }
    if name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal") {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "markdown" | "txt" | "rs" | "toml" | "json" | "sql" | "log")
    )
}

fn kind_for(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") | Some("markdown") => "markdown",
        _ => "code",
    }
}

fn language_for(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") | Some("markdown") => "markdown",
        Some("rs") => "rust",
        Some("sql") => "sql",
        Some("json") => "json",
        Some("toml") => "toml",
        _ => "text",
    }
}

fn pretty_body(body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string()),
        Err(_) => body.to_string(),
    }
}

fn modified(meta: &fs::Metadata) -> String {
    let secs = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format_unix(secs)
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
