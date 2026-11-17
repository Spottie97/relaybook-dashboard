use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSource {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
}

/// Find ledger files under the paths the operator passed.
///
/// A path may be a database file, a directory that contains `ledger.db` or
/// `.relaybook/ledger.db`, or a directory whose children are those ledgers.
/// The workspace id is the ledger's directory name, never the host path.
pub fn discover(inputs: &[PathBuf]) -> Vec<LedgerSource> {
    let mut found = Vec::new();
    for input in inputs {
        consider(input, &mut found);
    }
    let mut used = Vec::new();
    for source in &mut found {
        let mut id = source.id.clone();
        let mut n = 2;
        while used.iter().any(|existing: &String| existing == &id) {
            id = format!("{}-{n}", source.id);
            n += 1;
        }
        source.id = id.clone();
        source.label = id.clone();
        used.push(id);
    }
    found.sort_by(|left, right| left.label.cmp(&right.label));
    found
}

fn consider(path: &Path, out: &mut Vec<LedgerSource>) {
    if path.is_file() && is_db_file(path) {
        out.push(source_for_file(path));
        return;
    }
    if !path.is_dir() {
        return;
    }
    if let Some(db) = find_db(path) {
        out.push(LedgerSource {
            id: label_for_dir(path),
            label: label_for_dir(path),
            path: db,
        });
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    children.sort();
    for child in children {
        if child.is_file() && is_db_file(&child) {
            out.push(source_for_file(&child));
        } else if child.is_dir() {
            if let Some(db) = find_db(&child) {
                out.push(LedgerSource {
                    id: label_for_dir(&child),
                    label: label_for_dir(&child),
                    path: db,
                });
            }
        }
    }
}

fn find_db(dir: &Path) -> Option<PathBuf> {
    for relative in ["ledger.db", ".relaybook/ledger.db"] {
        let candidate = dir.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && is_db_file(&path) {
            files.push(path);
        }
    }
    if files.len() == 1 {
        files.pop()
    } else {
        None
    }
}

fn source_for_file(path: &Path) -> LedgerSource {
    let parent = path.parent().unwrap_or(path);
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("ledger");
    let label = if name == "ledger" {
        label_for_dir(parent)
    } else {
        sanitize(name)
    };
    LedgerSource {
        id: label.clone(),
        label,
        path: path.to_path_buf(),
    }
}

fn label_for_dir(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("ledger");
    if name == ".relaybook" {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .map(sanitize)
            .unwrap_or_else(|| sanitize(name))
    } else {
        sanitize(name)
    }
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "ledger".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_db_file(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("db") | Some("sqlite") | Some("sqlite3") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_of_ledgers_becomes_one_workspace_each() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("alpha")).unwrap();
        fs::create_dir_all(root.path().join("beta").join(".relaybook")).unwrap();
        fs::write(root.path().join("alpha").join("ledger.db"), b"").unwrap();
        fs::write(
            root.path()
                .join("beta")
                .join(".relaybook")
                .join("ledger.db"),
            b"",
        )
        .unwrap();
        let found = discover(&[root.path().to_path_buf()]);
        let labels: Vec<_> = found.iter().map(|source| source.label.as_str()).collect();
        assert_eq!(labels, ["alpha", "beta"]);
        assert!(found[1].path.ends_with(".relaybook/ledger.db"));
    }
}
