use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::model::NormEvent;

use super::parse::parse_line;
use super::rules::Rule;

const MAX_BACKLOG: u64 = 256 * 1024;

#[derive(Debug)]
struct Followed {
    path: PathBuf,
    agent: String,
    offset: u64,
    identity: u64,
    partial: String,
    source: String,
}

pub struct TailSet {
    pub rules: Vec<Rule>,
    followed: Vec<Followed>,
    pub unmapped: usize,
    seq: u64,
}

impl TailSet {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            followed: Vec::new(),
            unmapped: 0,
            seq: 1,
        }
    }

    pub fn known_agents(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .followed
            .iter()
            .map(|file| file.agent.clone())
            .collect();
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
            let relative = path.strip_prefix(root).unwrap_or(&path);
            match self
                .rules
                .iter()
                .find(|rule| rule.matcher.is_match(relative) || rule.matcher.is_match(&path))
            {
                Some(rule) => mapped.push((path, rule.agent.clone())),
                None => self.unmapped += 1,
            }
        }
        self.followed
            .retain(|file| mapped.iter().any(|(path, _)| path == &file.path));
        let mut events = Vec::new();
        for (path, agent) in mapped {
            if let Some(batch) = self.read_file(&path, &agent) {
                events.extend(batch);
            }
        }
        events
    }

    fn read_file(&mut self, path: &Path, agent: &str) -> Option<Vec<NormEvent>> {
        let meta = fs::metadata(path).ok()?;
        let identity = file_identity(&meta);
        let len = meta.len();
        let index = if let Some(index) = self.followed.iter().position(|file| file.path == path) {
            let slot = &mut self.followed[index];
            if slot.identity != identity || len < slot.offset {
                slot.identity = identity;
                slot.offset = 0;
                slot.partial.clear();
            }
            index
        } else {
            let start = if len > MAX_BACKLOG {
                len - MAX_BACKLOG
            } else {
                0
            };
            self.followed.push(Followed {
                path: path.to_path_buf(),
                agent: agent.to_string(),
                offset: start,
                identity,
                partial: String::new(),
                source: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "log".to_string()),
            });
            self.followed.len() - 1
        };
        let (lines, agent_name, source) = {
            let slot = &mut self.followed[index];
            if len < slot.offset {
                slot.offset = 0;
                slot.partial.clear();
            }
            let mut file = File::open(path).ok()?;
            if slot.offset == 0 && len > MAX_BACKLOG {
                slot.offset = len - MAX_BACKLOG;
            }
            file.seek(SeekFrom::Start(slot.offset)).ok()?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).ok()?;
            slot.offset += bytes.len() as u64;
            if slot.offset > MAX_BACKLOG
                && slot.partial.is_empty()
                && bytes.first().is_some_and(|byte| *byte != b'\n')
            {
                if let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') {
                    bytes.drain(..=newline);
                }
            }
            let text = String::from_utf8_lossy(&bytes);
            slot.partial.push_str(&text);
            let mut lines = Vec::new();
            while let Some(newline) = slot.partial.find('\n') {
                let line = slot.partial[..newline].trim_end_matches('\r').to_string();
                slot.partial.drain(..=newline);
                lines.push(line);
            }
            (lines, slot.agent.clone(), slot.source.clone())
        };
        let mut events = Vec::new();
        for line in lines {
            events.extend(parse_line(&agent_name, &line, &source, &mut self.seq));
        }
        Some(events)
    }
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

fn file_identity(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::logs::rules_from_config;

    #[test]
    fn partial_lines_wait_for_a_newline_and_rotation_rereads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude-planner.jsonl");
        let mut file = File::create(&path).unwrap();
        write!(file, "{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"hel").unwrap();
        file.flush().unwrap();
        let mut tails = TailSet::new(rules_from_config(&[]).unwrap());
        assert!(tails.poll(Some(dir.path())).is_empty());
        write!(file, "lo\"}}]}}}}\n").unwrap();
        file.flush().unwrap();
        let events = tails.poll(Some(dir.path()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "hello");
        drop(file);
        fs::remove_file(&path).unwrap();
        let mut rotated = File::create(&path).unwrap();
        writeln!(rotated, "again").unwrap();
        rotated.flush().unwrap();
        let events = tails.poll(Some(dir.path()));
        assert!(
            events.iter().any(|event| event.summary == "again"),
            "{events:?}"
        );
    }
}
