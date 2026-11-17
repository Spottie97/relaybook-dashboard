use globset::{GlobBuilder, GlobMatcher};

#[derive(Clone)]
pub struct Rule {
    pub agent: String,
    pub pattern: String,
    pub matcher: GlobMatcher,
}

pub fn default_patterns() -> Vec<(&'static str, &'static str)> {
    vec![
        ("claude", "claude-*-run.json"),
        ("claude", "**/claude-*-run.json"),
        ("cursor", "*-cursor-impl.json"),
        ("cursor", "**/*-cursor-impl.json"),
        ("cursor", "*-cursor-fix*.json"),
        ("cursor", "**/*-cursor-fix*.json"),
        ("codex", "*-codex-r*.md"),
        ("codex", "**/*-codex-r*.md"),
        ("codex", "codex-*-run.log"),
        ("codex", "**/codex-*-run.log"),
        ("graphify", "*-graphify-update.log"),
        ("graphify", "**/*-graphify-update.log"),
        ("graphify", "*-graphify-postcommit.log"),
        ("graphify", "**/*-graphify-postcommit.log"),
        ("cursor", "*-verify*.log"),
        ("cursor", "**/*-verify*.log"),
        ("cursor", "*-cargo-test.log"),
        ("cursor", "**/*-cargo-test.log"),
    ]
}

pub fn rules_from_config(custom: &[String]) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    if custom.is_empty() {
        for (agent, pattern) in default_patterns() {
            rules.push(compile(agent, pattern)?);
        }
        return Ok(rules);
    }
    for item in custom {
        let Some((agent, pattern)) = item.split_once('=') else {
            return Err(format!("log map '{item}' must look like agent=glob"));
        };
        if !known_agent(agent) {
            return Err(format!("unknown agent '{agent}' in log map"));
        }
        rules.push(compile(agent, pattern)?);
    }
    Ok(rules)
}

pub fn known_agent(agent: &str) -> bool {
    matches!(agent, "claude" | "cursor" | "codex" | "graphify")
}

fn compile(agent: &str, pattern: &str) -> Result<Rule, String> {
    let glob = GlobBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|err| format!("bad glob '{pattern}': {err}"))?;
    Ok(Rule {
        agent: agent.to_string(),
        pattern: pattern.to_string(),
        matcher: glob.compile_matcher(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_globs_match_milestone_names_and_skip_sidecars() {
        let rules = rules_from_config(&[]).unwrap();
        let hit = |name: &str| {
            rules
                .iter()
                .find(|rule| rule.matcher.is_match(name))
                .map(|rule| rule.agent.as_str())
        };
        assert_eq!(hit("claude-rev3-run.json"), Some("claude"));
        assert_eq!(hit("claude-r1-run.json"), Some("claude"));
        assert_eq!(hit("m1-cursor-impl.json"), Some("cursor"));
        assert_eq!(hit("r2-cursor-fix1.json"), Some("cursor"));
        assert_eq!(hit("Rev3-cursor-fix2.json"), Some("cursor"));
        assert_eq!(hit("nested/m1-cursor-impl.json"), Some("cursor"));
        assert_eq!(hit("m1-codex-r1.md"), Some("codex"));
        assert_eq!(hit("rev3-codex-review.md"), Some("codex"));
        assert_eq!(hit("codex-m1-run.log"), Some("codex"));
        assert_eq!(hit("codex-rev3-run.log"), Some("codex"));
        assert_eq!(hit("m1-graphify-update.log"), Some("graphify"));
        assert_eq!(hit("r1-graphify-postcommit.log"), Some("graphify"));
        assert_eq!(hit("m1-verify.log"), Some("cursor"));
        assert_eq!(hit("m1-verify-fmt.log"), Some("cursor"));
        assert_eq!(hit("r2-cargo-test.log"), Some("cursor"));
        assert_eq!(hit("orca-orchestrator.jsonl"), None);
        assert_eq!(hit("m1-cursor-impl-prompt.md"), None);
        assert_eq!(hit("m1-cursor-impl.result.md"), None);
        assert_eq!(hit("claude-rev3-run.meta"), None);
        assert_eq!(hit("claude-rev3-run.stderr"), None);
        assert!(rules_from_config(&["orca=**/*orca*".to_string()]).is_err());
    }
}
