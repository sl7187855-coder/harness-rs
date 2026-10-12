use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_POLICY_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyAction {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Debug)]
pub struct PolicyDecision {
    pub action: PolicyAction,
    pub reason: String,
    pub source: String,
    pub pattern: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RawRule {
    tool: String,
    #[serde(rename = "match", default = "wildcard")]
    matcher: String,
    action: String,
}
fn wildcard() -> String {
    "*".into()
}

#[derive(Default, Deserialize)]
struct RawPolicy {
    #[serde(default)]
    rules: Vec<RawRule>,
}

struct CompiledRule {
    tool: String,
    matcher_text: String,
    matcher: Option<GlobMatcher>,
    action: PolicyAction,
    source: String,
    specificity: usize,
}

#[derive(Default)]
pub struct PolicySet {
    rules: Vec<CompiledRule>,
    errors: Vec<String>,
}

impl PolicySet {
    pub fn load(home: &Path, workspace: &Path) -> Self {
        let mut result = Self::default();
        let workspace = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
        let sources = [
            (home.join("policy.toml"), "user".to_owned(), None),
            (
                workspace.join(".harness/policy.toml"),
                "project".to_owned(),
                Some(workspace.clone()),
            ),
        ];
        for (file, source, root) in sources {
            let metadata = match fs::symlink_metadata(&file) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    result
                        .errors
                        .push(format!("{source} policy metadata error: {e}"));
                    continue;
                }
            };
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_POLICY_BYTES
            {
                result.errors.push(format!(
                    "{source} policy must be a regular file under 64 KiB"
                ));
                continue;
            }
            if let Some(root) = root.as_ref() {
                match fs::canonicalize(&file) {
                    Ok(path) if path.starts_with(root) => {}
                    _ => {
                        result
                            .errors
                            .push("project policy escaped the workspace".into());
                        continue;
                    }
                }
            }
            let text = match fs::read_to_string(&file) {
                Ok(text) => text,
                Err(e) => {
                    result
                        .errors
                        .push(format!("{source} policy read error: {e}"));
                    continue;
                }
            };
            let parsed: RawPolicy = match toml::from_str(&text) {
                Ok(parsed) => parsed,
                Err(e) => {
                    result
                        .errors
                        .push(format!("{source} policy parse error: {e}"));
                    continue;
                }
            };
            for raw in parsed.rules.into_iter().take(512) {
                let tool = raw.tool.trim().to_owned();
                let matcher_text = raw.matcher.trim().to_owned();
                if tool.is_empty() || tool.len() > 80 || matcher_text.len() > 2048 {
                    result.errors.push(format!(
                        "{source} policy has an invalid tool or match length"
                    ));
                    continue;
                }
                let action = match raw.action.to_ascii_lowercase().as_str() {
                    "allow" => PolicyAction::Allow,
                    "ask" => PolicyAction::Ask,
                    "deny" => PolicyAction::Deny,
                    _ => {
                        result
                            .errors
                            .push(format!("{source} policy has an invalid action"));
                        continue;
                    }
                };
                let matcher = if matcher_text == "*" || matcher_text.is_empty() {
                    None
                } else {
                    match GlobBuilder::new(&matcher_text)
                        .case_insensitive(false)
                        .literal_separator(false)
                        .build()
                    {
                        Ok(glob) => Some(glob.compile_matcher()),
                        Err(e) => {
                            result.errors.push(format!("{source} invalid glob: {e}"));
                            continue;
                        }
                    }
                };
                let specificity = tool.len()
                    + if matcher_text == "*" {
                        0
                    } else {
                        matcher_text.len()
                    };
                result.rules.push(CompiledRule {
                    tool,
                    matcher_text,
                    matcher,
                    action,
                    source: source.clone(),
                    specificity,
                });
            }
        }
        result
    }

    /// A malformed policy fails closed for side-effecting tools; it never turns a
    /// parse failure into permission. Explicit deny always outranks ask/allow.
    pub fn evaluate(
        &self,
        tool: &str,
        candidate: &str,
        side_effecting: bool,
    ) -> Option<PolicyDecision> {
        let mut matched: Vec<&CompiledRule> = self
            .rules
            .iter()
            .filter(|rule| {
                (rule.tool == "*" || rule.tool == tool)
                    && rule
                        .matcher
                        .as_ref()
                        .is_none_or(|matcher| matcher.is_match(candidate))
            })
            .collect();
        if !self.errors.is_empty() {
            // Malformed/unsupported rules must never neutralize a valid explicit
            // deny that still matched this operation.
            if let Some(rule) = matched
                .iter()
                .filter(|rule| rule.action == PolicyAction::Deny)
                .max_by_key(|rule| rule.specificity)
            {
                return Some(PolicyDecision {
                    action: PolicyAction::Deny,
                    reason: format!(
                        "{} policy rule: {} {} denies operation (other policy errors detected)",
                        rule.source, rule.tool, rule.matcher_text
                    ),
                    source: rule.source.clone(),
                    pattern: rule.matcher_text.clone(),
                });
            }
            return side_effecting.then(|| PolicyDecision {
                action: PolicyAction::Ask,
                reason: format!(
                    "policy invalid; explicit approval required ({})",
                    self.errors.join("; ").chars().take(240).collect::<String>()
                ),
                source: "policy-error".into(),
                pattern: String::new(),
            });
        }
        if matched.is_empty() {
            return None;
        }
        matched.sort_by_key(|rule| {
            (
                match rule.action {
                    PolicyAction::Deny => 0,
                    PolicyAction::Ask => 1,
                    PolicyAction::Allow => 2,
                },
                std::cmp::Reverse(rule.specificity),
                if rule.source == "project" { 0 } else { 1 },
            )
        });
        let chosen = matched[0];
        let action =
            if side_effecting && chosen.action == PolicyAction::Allow && chosen.source == "project"
            {
                PolicyAction::Ask
            } else {
                chosen.action
            };
        let verb = match action {
            PolicyAction::Allow => "allows",
            PolicyAction::Ask => "requires approval for",
            PolicyAction::Deny => "denies",
        };
        Some(PolicyDecision {
            action,
            reason: format!(
                "{} policy rule: {} {} {}",
                chosen.source, chosen.tool, chosen.matcher_text, verb
            ),
            source: chosen.source.clone(),
            pattern: chosen.matcher_text.clone(),
        })
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.errors
    }
}

pub fn policy_paths(home: &Path, workspace: &Path) -> [PathBuf; 2] {
    [
        home.join("policy.toml"),
        workspace.join(".harness/policy.toml"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn deny_has_precedence_over_specific_allow() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let workspace = temp.path().join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(workspace.join(".harness")).unwrap();
        fs::write(
            home.join("policy.toml"),
            r#"
[[rules]]
tool = "Bash"
match = "rm -rf *"
action = "deny"
"#,
        )
        .unwrap();
        fs::write(
            workspace.join(".harness/policy.toml"),
            r#"
[[rules]]
tool = "Bash"
match = "*"
action = "allow"
"#,
        )
        .unwrap();
        let policy = PolicySet::load(&home, &workspace);
        assert_eq!(
            policy
                .evaluate("Bash", "rm -rf build", true)
                .unwrap()
                .action,
            PolicyAction::Deny
        );
        assert_eq!(
            policy.evaluate("Bash", "git status", true).unwrap().action,
            PolicyAction::Ask,
            "repository-controlled policy must not silently authorize side effects"
        );
    }

    #[test]
    fn project_specific_ask_beats_broad_allow_but_not_deny() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let workspace = temp.path().join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(workspace.join(".harness")).unwrap();
        fs::write(
            home.join("policy.toml"),
            r#"
[[rules]]
tool = "Bash"
match = "*"
action = "allow"
"#,
        )
        .unwrap();
        fs::write(
            workspace.join(".harness/policy.toml"),
            r#"
[[rules]]
tool = "Bash"
match = "git push*"
action = "ask"
"#,
        )
        .unwrap();
        let policy = PolicySet::load(&home, &workspace);
        assert_eq!(
            policy
                .evaluate("Bash", "git push origin main", true)
                .unwrap()
                .action,
            PolicyAction::Ask
        );
    }

    #[test]
    fn malformed_policy_cannot_accidentally_allow_side_effects() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("policy.toml"), "[[rules]\naction =").unwrap();
        let policy = PolicySet::load(&home, temp.path());
        assert_eq!(
            policy.evaluate("Bash", "rm thing", true).unwrap().action,
            PolicyAction::Ask
        );
        assert!(policy.evaluate("Read", "main.rs", false).is_none());
    }

    #[test]
    fn invalid_policy_entries_do_not_neutralize_matching_explicit_denies() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(
            home.join("policy.toml"),
            r#"
[[rules]]
tool = "Bash"
match = "rm -rf *"
action = "deny"

[[rules]]
tool = "Bash"
match = "*"
action = "unsupported"
"#,
        )
        .unwrap();
        let policy = PolicySet::load(&home, temp.path());
        assert_eq!(
            policy
                .evaluate("Bash", "rm -rf build", true)
                .unwrap()
                .action,
            PolicyAction::Deny
        );
        assert_eq!(
            policy.evaluate("Bash", "git status", true).unwrap().action,
            PolicyAction::Ask
        );
    }
}
