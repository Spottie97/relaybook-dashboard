use globset::{Glob, GlobMatcher};

#[derive(Clone)]
pub struct Rule {
    pub agent: String,
    pub pattern: String,
    pub matcher: GlobMatcher,
}

pub fn default_patterns() -> Vec<(&'static str, &'static str)> {
    vec![
        ("orca", "**/*orca*"),
        ("graphify", "**/*graphify*"),
        ("codex", "**/*codex*"),
        ("cursor", "**/*cursor*"),
        ("claude", "**/*claude*"),
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
    matches!(agent, "claude" | "cursor" | "codex" | "graphify" | "orca")
}

fn compile(agent: &str, pattern: &str) -> Result<Rule, String> {
    let glob = Glob::new(pattern).map_err(|err| format!("bad glob '{pattern}': {err}"))?;
    Ok(Rule {
        agent: agent.to_string(),
        pattern: pattern.to_string(),
        matcher: glob.compile_matcher(),
    })
}
