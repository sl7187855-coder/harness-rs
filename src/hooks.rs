use crate::{
    sandbox::{PreparedSandbox, Profile},
    types::AppState,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};

const MAX_HOOK_CONFIG: u64 = 64 * 1024;
const MAX_HOOK_STDIO: usize = 64 * 1024;
const EVENTS: &[&str] = &[
    "session_start",
    "session_end",
    "user_prompt_submit",
    "pre_tool_use",
    "permission_request",
    "post_tool_use",
    "pre_compact",
    "post_compact",
    "subagent_start",
    "subagent_stop",
    "memory_write",
    "stop",
];

#[derive(Debug, Deserialize)]
struct HookFile {
    #[serde(default)]
    hooks: Vec<HookEntry>,
}
#[derive(Clone, Debug, Deserialize)]
struct HookEntry {
    event: String,
    #[serde(default = "wildcard")]
    matcher: String,
    command: String,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
}
fn wildcard() -> String {
    ".*".into()
}
fn default_timeout() -> u64 {
    10
}

#[derive(Clone, Debug, Default)]
pub struct HookOutcome {
    pub additional_context: Vec<String>,
    pub updated_input: Option<Value>,
    pub denied: Option<String>,
    pub asked: Option<String>,
    pub ran: usize,
}

fn project_hook_path(workspace: &std::path::Path) -> PathBuf {
    workspace.join(".harness/hooks.toml")
}

fn load_hook_files(
    home: &std::path::Path,
    workspace: &std::path::Path,
) -> Result<Vec<(String, HookEntry)>, String> {
    let canonical_workspace =
        fs::canonicalize(workspace).map_err(|e| format!("workspace unavailable for hooks: {e}"))?;
    let files = [
        (home.join("hooks.toml"), "user"),
        (project_hook_path(&canonical_workspace), "project"),
    ];
    let mut hooks = Vec::new();
    for (path, source) in files {
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("hook configuration metadata failed: {e}")),
        };
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > MAX_HOOK_CONFIG {
            return Err(format!(
                "{source} hooks.toml must be a regular file under 64 KiB"
            ));
        }
        if source == "project"
            && !fs::canonicalize(&path).is_ok_and(|p| p.starts_with(&canonical_workspace))
        {
            return Err("project hook configuration escaped the workspace".into());
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("{source} hooks.toml read failed: {e}"))?;
        let parsed: HookFile = toml::from_str(&content)
            .map_err(|e| format!("{source} hooks.toml parse error: {e}"))?;
        if parsed.hooks.len() > 256 {
            return Err("at most 256 hooks can be configured".into());
        }
        for hook in parsed.hooks {
            if !EVENTS.contains(&hook.event.as_str()) {
                return Err(format!("unsupported hook event: {}", hook.event));
            }
            if hook.command.trim().is_empty() || hook.command.len() > 4096 {
                return Err("hook command must contain 1–4096 bytes".into());
            }
            if hook.matcher.len() > 512 {
                return Err("hook matcher exceeds 512 bytes".into());
            }
            if regex::Regex::new(&hook.matcher).is_err() {
                return Err(format!("invalid hook matcher for {}", hook.event));
            }
            hooks.push((source.to_owned(), hook));
        }
    }
    Ok(hooks)
}

/// Run a configured event through the read-only sandbox. Security events are
/// fail-closed: invalid config, nonzero exit, malformed JSON or timeout deny.
pub async fn run(
    state: &Arc<AppState>,
    event: &str,
    tool_name: &str,
    tool_input: &Value,
    session: &str,
) -> Result<HookOutcome, String> {
    if !EVENTS.contains(&event) {
        return Err(format!("unsupported hook event: {event}"));
    }
    let workspace = state.tools.workspace_for_session(session);
    let home = std::env::var_os("HARNESS_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            state
                .config
                .data_dir
                .parent()
                .map(std::path::Path::to_path_buf)
        })
        .unwrap_or_else(|| PathBuf::from("/var/lib/harness-rs"));
    let hooks = load_hook_files(&home, &workspace)?;
    let mut outcome = HookOutcome::default();
    for (source, hook) in hooks.into_iter().filter(|(_, h)| h.event == event) {
        let matcher =
            regex::Regex::new(&hook.matcher).map_err(|e| format!("hook matcher failed: {e}"))?;
        if !matcher.is_match(tool_name) {
            continue;
        }
        outcome.ran += 1;
        let payload = json!({
            "session_id":session,
            "hook_event_name":event,
            "tool_name":tool_name,
            "tool_input":tool_input,
            "cwd":workspace,
            "source":source
        });
        let result = execute(
            &workspace,
            &hook.command,
            &payload,
            hook.timeout_seconds.min(10),
        )
        .await;
        let (code, stdout, stderr) = match result {
            Ok(r) => r,
            Err(error) if security_event(event) => {
                return Err(format!("security hook failed closed: {error}"));
            }
            Err(error) => {
                tracing::warn!(event,tool=%tool_name,error=%error,"observational hook failed");
                continue;
            }
        };
        let decision_value = match serde_json::from_str::<Value>(&stdout) {
            Ok(value) => value,
            Err(error) => {
                if let Some(denial) = malformed_security_output_error(event, &error.to_string()) {
                    return Err(denial);
                }
                tracing::warn!(event,error=%error,"observational hook returned invalid JSON");
                continue;
            }
        };
        if code == 2 {
            return Err(deny_reason(&decision_value, &stderr));
        }
        if code != 0 {
            if security_event(event) {
                return Err(format!(
                    "security hook exited with status {code}; denied fail-closed"
                ));
            }
            tracing::warn!(event,tool=%tool_name,exit=code,"observational hook exited nonzero and was ignored");
            continue;
        }
        apply_decision(event, &decision_value, &mut outcome)?;
    }
    Ok(outcome)
}

fn security_event(event: &str) -> bool {
    ["pre_tool_use", "permission_request", "user_prompt_submit"].contains(&event)
}

fn malformed_security_output_error(event: &str, error: &str) -> Option<String> {
    security_event(event).then(|| format!("security hook returned invalid JSON; denied: {error}"))
}

fn deny_reason(value: &Value, stderr: &str) -> String {
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or(stderr)
        .chars()
        .take(1200)
        .collect::<String>();
    if reason.is_empty() {
        "denied by hook".to_owned()
    } else {
        reason
    }
}

fn apply_decision(event: &str, value: &Value, outcome: &mut HookOutcome) -> Result<(), String> {
    match value
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("allow")
    {
        "allow" => {}
        "ask" => {
            outcome.asked = Some(
                value
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("hook requests approval")
                    .chars()
                    .take(1200)
                    .collect(),
            )
        }
        "deny" => {
            return Err(value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("denied by hook")
                .chars()
                .take(1200)
                .collect());
        }
        other if security_event(event) => {
            return Err(format!(
                "security hook returned unsupported decision {other}; denied"
            ));
        }
        _ => {}
    }
    if let Some(context) = value.get("additionalContext").and_then(Value::as_str) {
        if context.len() <= 8_000 {
            outcome.additional_context.push(context.to_owned());
        }
    }
    if let Some(updated) = value.get("updatedInput").filter(|v| v.is_object()) {
        if serde_json::to_vec(updated).is_ok_and(|bytes| bytes.len() <= 64 * 1024) {
            outcome.updated_input = Some(updated.clone());
        }
    }
    Ok(())
}

async fn read_capped<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    cap: usize,
) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut limited = reader.take((cap + 1) as u64);
    limited.read_to_end(&mut bytes).await?;
    if bytes.len() > cap {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "hook output exceeded 64 KiB",
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

async fn execute(
    workspace: &std::path::Path,
    command_text: &str,
    payload: &Value,
    timeout_seconds: u64,
) -> Result<(i32, String, String), String> {
    let sandbox = PreparedSandbox::prepare(workspace, Profile::ReadOnly)
        .map_err(|e| format!("hook sandbox setup failed closed: {e}"))?;
    let mut cmd = Command::new("/bin/bash");
    // Put the child in a dedicated process group before the sandbox's seccomp
    // filter is installed, so timeout cleanup can kill the shell and all children.
    cmd.args(["--noprofile", "--norc", "-lc", command_text])
        .process_group(0)
        .current_dir(workspace)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", sandbox.temp_dir())
        .env("TMPDIR", sandbox.temp_dir())
        .env("LANG", "C.UTF-8")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    sandbox.attach(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("hook spawn failed: {e}"))?;
    let pid = child.id().unwrap_or(0);
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "hook stdin unavailable".to_owned())?;
    let input =
        serde_json::to_vec(payload).map_err(|e| format!("hook input serialization failed: {e}"))?;
    let writer = tokio::spawn(async move {
        let result = stdin.write_all(&input).await;
        drop(stdin);
        result
    });
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "hook stdout unavailable".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "hook stderr unavailable".to_owned())?;
    let out_task = tokio::spawn(read_capped(stdout, MAX_HOOK_STDIO));
    let err_task = tokio::spawn(read_capped(stderr, MAX_HOOK_STDIO));
    match timeout(Duration::from_secs(timeout_seconds.max(1)), child.wait()).await {
        Ok(Ok(status)) => {
            writer
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let out = out_task
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let err = err_task
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            Ok((status.code().unwrap_or(-1), out, err))
        }
        Ok(Err(e)) => Err(format!("hook wait failed: {e}")),
        Err(_) => {
            if pid > 0 {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", "--", &format!("-{pid}")])
                    .status()
                    .await;
            }
            let _ = child.kill().await;
            Err(format!(
                "hook exceeded {} second timeout (process group {pid})",
                timeout_seconds.max(1)
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_events_are_whitelisted() {
        assert!(security_event("pre_tool_use"));
        assert!(security_event("permission_request"));
        assert!(security_event("user_prompt_submit"));
        assert!(!security_event("post_tool_use"));
    }

    #[test]
    fn hook_config_rejects_unknown_events_and_invalid_matchers() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path().join("workspace");
        fs::create_dir_all(&work).unwrap();
        fs::write(
            temp.path().join("hooks.toml"),
            r#"
[[hooks]]
event = "made_up"
matcher = ".*"
command = "true"
"#,
        )
        .unwrap();
        let error = load_hook_files(temp.path(), &work).unwrap_err();
        assert!(error.contains("unsupported hook event"));
    }

    #[test]
    fn project_hook_config_rejects_symlink_to_outside_workspace() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(workspace.join(".harness")).unwrap();

        let outside = root.path().join("outside-hooks.toml");
        fs::write(&outside, "hooks = []\n").unwrap();
        symlink(&outside, workspace.join(".harness/hooks.toml")).unwrap();

        let error = load_hook_files(&home, &workspace).unwrap_err();
        assert!(
            error.contains("regular file"),
            "symlinked project hook config must fail closed: {error}"
        );
    }

    #[tokio::test]
    async fn hook_execution_uses_read_only_sandbox_and_passes_json_stdin() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path().join("workspace");
        fs::create_dir_all(&work).unwrap();
        let payload = json!({"tool_name":"Read","session_id":"hook-test"});
        let command = "python3 -c 'import json,sys; d=json.load(sys.stdin); print(json.dumps({\"decision\":\"allow\",\"additionalContext\":d[\"tool_name\"]+\"-safe\"}))'";
        let (code, stdout, _) = execute(&work, command, &payload, 4).await.unwrap();
        assert_eq!(code, 0);
        let output: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(output["decision"], "allow");
        assert_eq!(output["additionalContext"], "Read-safe");
    }

    #[test]
    fn security_hook_malformed_json_is_fail_closed_but_observational_hook_is_not() {
        assert!(
            malformed_security_output_error("pre_tool_use", "invalid json")
                .unwrap()
                .contains("denied")
        );
        assert!(malformed_security_output_error("permission_request", "invalid json").is_some());
        assert!(malformed_security_output_error("post_tool_use", "invalid json").is_none());
    }

    #[test]
    fn hook_ask_and_deny_decisions_are_interpreted_explicitly() {
        let mut outcome = HookOutcome::default();
        apply_decision(
            "pre_tool_use",
            &json!({"decision":"ask","reason":"confirm first"}),
            &mut outcome,
        )
        .unwrap();
        assert_eq!(outcome.asked.as_deref(), Some("confirm first"));
        let denied = apply_decision(
            "pre_tool_use",
            &json!({"decision":"deny","reason":"blocked"}),
            &mut outcome,
        )
        .unwrap_err();
        assert_eq!(denied, "blocked");
        let unsupported =
            apply_decision("pre_tool_use", &json!({"decision":"maybe"}), &mut outcome).unwrap_err();
        assert!(unsupported.contains("unsupported decision"));
        // Non-security/observational hooks may safely ignore an unknown decision.
        apply_decision("post_tool_use", &json!({"decision":"maybe"}), &mut outcome).unwrap();
    }

    #[test]
    fn hook_denial_reason_is_bounded_and_has_safe_fallback() {
        assert_eq!(deny_reason(&json!({"reason":"no"}), "stderr"), "no");
        assert_eq!(deny_reason(&json!({}), "stderr"), "stderr");
        assert_eq!(deny_reason(&json!({}), ""), "denied by hook");
        assert_eq!(
            deny_reason(&json!({"reason":"x".repeat(1400)}), "").len(),
            1200
        );
    }

    fn process_group_has_member(group_id: u32) -> bool {
        let Ok(entries) = fs::read_dir("/proc") else {
            return false;
        };
        entries.flatten().any(|entry| {
            if !entry
                .file_name()
                .to_string_lossy()
                .chars()
                .all(|c| c.is_ascii_digit())
            {
                return false;
            }
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                return false;
            };
            let Some((_, tail)) = stat.rsplit_once(')') else {
                return false;
            };
            let fields = tail.split_whitespace().collect::<Vec<_>>();
            fields.get(2).and_then(|value| value.parse::<u32>().ok()) == Some(group_id)
        })
    }

    #[tokio::test]
    async fn hook_timeout_terminates_the_entire_process_group() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path().join("workspace");
        fs::create_dir_all(&work).unwrap();
        let started = std::time::Instant::now();
        let error = execute(&work, "/bin/sleep 30 & wait", &json!({}), 1)
            .await
            .unwrap_err();
        assert!(
            error.contains("timeout"),
            "unexpected timeout error: {error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "hook timeout took too long"
        );
        let group_id = error
            .split("process group ")
            .nth(1)
            .and_then(|value| value.split(')').next())
            .and_then(|value| value.parse::<u32>().ok())
            .expect("timeout error must identify the tested process group");
        let mut alive = false;
        for _ in 0..20 {
            alive = process_group_has_member(group_id);
            if !alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!alive, "timed-out hook left a process in group {group_id}");
    }
}
