//! Minimal, fail-closed MCP stdio resources client.
//! Configured commands are launched without a shell inside a read-only Landlock
//! + seccomp sandbox. Network syscalls remain denied.
use crate::sandbox::{PreparedSandbox, Profile};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_RPC_BYTES: usize = 1024 * 1024;
const RPC_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_SERVERS: usize = 20;

#[derive(Clone, Debug, Deserialize)]
struct ServerConfig {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}
fn enabled_by_default() -> bool {
    true
}
#[derive(Default, Deserialize)]
struct ServerConfigFile {
    #[serde(default)]
    servers: Vec<ServerConfig>,
}

fn load_servers(data_dir: &Path) -> Result<Vec<ServerConfig>, String> {
    let path = data_dir.join("mcp-servers.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot inspect MCP config: {error}")),
    };
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > MAX_CONFIG_BYTES {
        return Err(
            "mcp-servers.json must be a regular non-symlink file no larger than 64 KiB".into(),
        );
    }
    let text =
        fs::read_to_string(&path).map_err(|error| format!("cannot read MCP config: {error}"))?;
    let parsed: ServerConfigFile =
        serde_json::from_str(&text).map_err(|error| format!("invalid MCP config: {error}"))?;
    if parsed.servers.len() > MAX_SERVERS {
        return Err(format!(
            "MCP config is limited to {MAX_SERVERS} server entries"
        ));
    }
    let mut names = std::collections::HashSet::new();
    let mut enabled = Vec::new();
    for mut server in parsed.servers {
        server.name = server.name.trim().to_owned();
        if server.name.is_empty()
            || server.name.len() > 96
            || !server
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(
                "MCP server names must be 1–96 ASCII letters/digits/dot/underscore/hyphen".into(),
            );
        }
        if !names.insert(server.name.clone()) {
            return Err(format!("duplicate MCP server name: {}", server.name));
        }
        if server.command.len() > 1024
            || server.args.len() > 32
            || server.args.iter().any(|arg| arg.len() > 2048)
        {
            return Err(format!(
                "MCP command/args exceed limits for {}",
                server.name
            ));
        }
        if !server.command.starts_with('/') {
            return Err(format!(
                "MCP command for {} must be an absolute executable path",
                server.name
            ));
        }
        let command = fs::canonicalize(&server.command)
            .map_err(|_| format!("MCP executable is unavailable for {}", server.name))?;
        let file_meta = fs::metadata(&command)
            .map_err(|_| format!("MCP executable is unavailable for {}", server.name))?;
        if !file_meta.is_file() {
            return Err(format!(
                "MCP command is not a regular file for {}",
                server.name
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if file_meta.permissions().mode() & 0o111 == 0 {
                return Err(format!("MCP command is not executable for {}", server.name));
            }
        }
        server.command = command.to_string_lossy().to_string();
        if server.enabled {
            enabled.push(server);
        }
    }
    Ok(enabled)
}

async fn read_capped_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    cap: usize,
) -> Result<String, String> {
    let mut bytes = Vec::new();
    loop {
        let (take, done) = {
            let buffer = reader
                .fill_buf()
                .await
                .map_err(|_| "MCP server closed its stdout".to_owned())?;
            if buffer.is_empty() {
                if bytes.is_empty() {
                    return Err("MCP server closed stdout before replying".into());
                }
                return Ok(String::from_utf8_lossy(&bytes).to_string());
            }
            let end = buffer.iter().position(|byte| *byte == b'\n');
            (
                end.map(|index| index + 1).unwrap_or(buffer.len()),
                end.is_some(),
            )
        };
        if bytes.len().saturating_add(take) > cap {
            return Err(format!("MCP response exceeded {cap} bytes"));
        }
        {
            let buffer = reader
                .fill_buf()
                .await
                .map_err(|_| "MCP server stdout failed".to_owned())?;
            bytes.extend_from_slice(&buffer[..take]);
        }
        reader.consume(take);
        if done {
            return Ok(String::from_utf8_lossy(&bytes)
                .trim_end_matches(['\r', '\n'])
                .to_owned());
        }
    }
}

async fn write_message(
    child_stdin: &mut tokio::process::ChildStdin,
    message: &Value,
) -> Result<(), String> {
    let encoded =
        serde_json::to_vec(message).map_err(|_| "cannot encode MCP request".to_owned())?;
    if encoded.len() > MAX_RPC_BYTES {
        return Err("MCP request exceeds 1 MiB".into());
    }
    child_stdin
        .write_all(&encoded)
        .await
        .map_err(|_| "cannot write to MCP server stdin".to_owned())?;
    child_stdin
        .write_all(b"\n")
        .await
        .map_err(|_| "cannot terminate MCP request".to_owned())?;
    child_stdin
        .flush()
        .await
        .map_err(|_| "cannot flush MCP request".to_owned())
}

async fn read_response<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    expected_id: u64,
) -> Result<Value, String> {
    for _ in 0..64 {
        let raw = timeout(RPC_TIMEOUT, read_capped_line(reader, MAX_RPC_BYTES))
            .await
            .map_err(|_| "MCP response timed out".to_owned())??;
        let value: Value = serde_json::from_str(&raw)
            .map_err(|_| "MCP server sent invalid JSON-RPC".to_owned())?;
        if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err("MCP server sent an invalid JSON-RPC version".into());
        }
        if value.get("id").and_then(Value::as_u64) == Some(expected_id) {
            if let Some(error) = value.get("error") {
                let code = error.get("code").and_then(Value::as_i64).unwrap_or(-1);
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP request failed");
                return Err(format!(
                    "MCP error {code}: {}",
                    message.chars().take(300).collect::<String>()
                ));
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "MCP response has no result".into());
        }
    }
    Err("MCP server exceeded the 64-message handshake limit".into())
}

struct RunningServer {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    _sandbox: std::sync::Arc<PreparedSandbox>,
}

impl RunningServer {
    async fn start(config: &ServerConfig, workspace: &Path) -> Result<Self, String> {
        let sandbox = PreparedSandbox::prepare(workspace, Profile::ReadOnly).map_err(|error| {
            format!("MCP sandbox unavailable; refusing to run unsandboxed: {error}")
        })?;
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(workspace)
            .env_clear()
            .env("PATH", "/opt/node-24.21.0/bin:/usr/local/bin:/usr/bin:/bin")
            .env("HOME", sandbox.temp_dir())
            .env("TMPDIR", sandbox.temp_dir())
            .env("LANG", "C.UTF-8")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        sandbox.attach(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start MCP server {}: {error}", config.name))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "MCP stdin unavailable".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "MCP stdout unavailable".to_owned())?;
        let mut server = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            _sandbox: sandbox,
        };
        server.initialize().await?;
        Ok(server)
    }

    async fn initialize(&mut self) -> Result<(), String> {
        write_message(&mut self.stdin,&json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"harness-rs","version":env!("CARGO_PKG_VERSION")}}
        })).await?;
        let result = read_response(&mut self.stdout, 1).await?;
        if result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err("MCP server initialize result is missing protocolVersion".into());
        }
        write_message(
            &mut self.stdin,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await?;
        Ok(())
    }

    async fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value, String> {
        write_message(
            &mut self.stdin,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .await?;
        read_response(&mut self.stdout, id).await
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        // Tokio's kill_on_drop terminates the child. Explicit child cleanup occurs
        // in the awaited helper when a normal response has already been read.
        self.child.start_kill().ok();
    }
}

pub async fn wait_for_servers(data_dir: &Path, workspace: &Path) -> Result<Value, String> {
    let servers = load_servers(data_dir)?;
    if servers.is_empty() {
        return Ok(
            json!({"ok":true,"configured":0,"ready":0,"servers":[],"detail":"No MCP servers are configured in mcp-servers.json."}),
        );
    }
    let mut outcomes = Vec::new();
    for server in servers {
        let name = server.name.clone();
        let result = RunningServer::start(&server, workspace).await;
        match result {
            Ok(_running)=>outcomes.push(json!({"name":name,"ready":true})),
            Err(error)=>outcomes.push(json!({"name":name,"ready":false,"error":error.chars().take(300).collect::<String>()})),
        }
    }
    let ready = outcomes.iter().filter(|item| item["ready"] == true).count();
    Ok(
        json!({"ok":ready==outcomes.len(),"configured":outcomes.len(),"ready":ready,"servers":outcomes}),
    )
}

pub async fn list_resources(
    data_dir: &Path,
    workspace: &Path,
    server_filter: Option<&str>,
    limit: usize,
) -> Result<Value, String> {
    let servers = load_servers(data_dir)?;
    let filtered = servers
        .into_iter()
        .filter(|server| server_filter.is_none_or(|name| name == server.name))
        .collect::<Vec<_>>();
    if let Some(filter) = server_filter {
        if !filtered.iter().any(|server| server.name == filter) {
            return Err(format!(
                "MCP server '{filter}' is not configured/enabled; configure it in mcp-servers.json first"
            ));
        }
    }
    let mut items = Vec::new();
    for server in filtered {
        let server_name = server.name.clone();
        let result = async {
            let mut client = RunningServer::start(&server, workspace).await?;
            client.request(2, "resources/list", json!({})).await
        }
        .await;
        match result {
            Ok(result) => {
                let resources = result
                    .get("resources")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for item in resources.into_iter().take(100) {
                    let uri = item.get("uri").and_then(Value::as_str).unwrap_or("");
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                    if uri.is_empty() || uri.len() > 2000 || name.len() > 300 {
                        continue;
                    }
                    items.push(json!({
                        "server":server_name,
                        "uri":uri,
                        "name":name,
                        "description":item.get("description").and_then(Value::as_str).unwrap_or("").chars().take(300).collect::<String>(),
                        "mimeType":item.get("mimeType").and_then(Value::as_str).unwrap_or(""),
                    }));
                }
            }
            Err(error) => {
                if server_filter.is_some() {
                    return Err(error);
                }
                items.push(json!({"server":server_name,"error":error.chars().take(300).collect::<String>()}));
            }
        }
    }
    items.truncate(limit.clamp(1, 50));
    Ok(
        json!({"ok":true,"count":items.iter().filter(|item|item.get("uri").is_some()).count(),"resources":items}),
    )
}

pub async fn read_resource(
    data_dir: &Path,
    workspace: &Path,
    server_name: &str,
    uri: &str,
) -> Result<Value, String> {
    if uri.trim().is_empty() || uri.len() > 2000 {
        return Err("MCP resource URI must contain 1–2000 bytes".into());
    }
    let servers = load_servers(data_dir)?;
    let server = servers
        .into_iter()
        .find(|server| server.name == server_name)
        .ok_or_else(|| format!("MCP server '{server_name}' is not configured/enabled"))?;
    let mut client = RunningServer::start(&server, workspace).await?;
    let result = client
        .request(2, "resources/read", json!({"uri":uri}))
        .await?;
    let contents = result
        .get("contents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut output = Vec::new();
    for item in contents.into_iter().take(32) {
        let mime = item.get("mimeType").and_then(Value::as_str).unwrap_or("");
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            output.push(json!({"uri":item.get("uri").and_then(Value::as_str).unwrap_or(uri),"mimeType":mime,"text":text.chars().take(20_000).collect::<String>()}));
        } else if item.get("blob").and_then(Value::as_str).is_some() {
            output.push(json!({"uri":item.get("uri").and_then(Value::as_str).unwrap_or(uri),"mimeType":mime,"binary":true,"detail":"Binary MCP resource omitted from the model result; use the resource in an appropriate file workflow."}));
        }
    }
    Ok(json!({"ok":true,"server":server_name,"uri":uri,"count":output.len(),"contents":output}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_mcp_configuration_is_an_empty_safe_state() {
        let temp = tempfile::tempdir().unwrap();
        assert!(load_servers(temp.path()).unwrap().is_empty());
    }

    #[test]
    fn mcp_configuration_rejects_duplicates_and_relative_commands() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("mcp-servers.json"),
            json!({"servers":[
                {"name":"x","command":"python3"},
                {"name":"x","command":"/usr/bin/python3"}
            ]})
            .to_string(),
        )
        .unwrap();
        assert!(load_servers(temp.path()).unwrap_err().contains("absolute"));
        fs::write(
            temp.path().join("mcp-servers.json"),
            json!({"servers":[
                {"name":"x","command":"/usr/bin/python3"},
                {"name":"x","command":"/usr/bin/python3"}
            ]})
            .to_string(),
        )
        .unwrap();
        assert!(load_servers(temp.path()).unwrap_err().contains("duplicate"));
    }

    #[tokio::test]
    async fn mock_mcp_server_list_and_read_resources_roundtrip() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        let script = workspace.join("mock_mcp.py");
        fs::write(&script,r#"import json,sys
for line in sys.stdin:
    try: m=json.loads(line)
    except Exception: continue
    method=m.get("method"); ident=m.get("id")
    if method=="initialize":
        out={"jsonrpc":"2.0","id":ident,"result":{"protocolVersion":"2025-06-18","capabilities":{"resources":{}},"serverInfo":{"name":"mock","version":"1"}}}
    elif method=="resources/list":
        out={"jsonrpc":"2.0","id":ident,"result":{"resources":[{"uri":"mock://readme","name":"Readme","mimeType":"text/plain"}]}}
    elif method=="resources/read":
        out={"jsonrpc":"2.0","id":ident,"result":{"contents":[{"uri":m["params"]["uri"],"mimeType":"text/plain","text":"MOCK-MCP-RESOURCE"}]}}
    elif ident is not None:
        out={"jsonrpc":"2.0","id":ident,"result":{}}
    else: continue
    print(json.dumps(out),flush=True)
"#).unwrap();
        let config = json!({"servers":[{"name":"mock","command":"/usr/bin/python3","args":[script.to_string_lossy()],"enabled":true}]});
        fs::write(data.join("mcp-servers.json"), config.to_string()).unwrap();
        let resources = list_resources(&data, &workspace, None, 20).await.unwrap();
        assert_eq!(resources["count"], 1);
        assert_eq!(resources["resources"][0]["uri"], "mock://readme");
        let read = read_resource(&data, &workspace, "mock", "mock://readme")
            .await
            .unwrap();
        assert_eq!(read["contents"][0]["text"], "MOCK-MCP-RESOURCE");
    }

    #[test]
    fn mcp_configuration_rejects_symlinked_config_file() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        fs::create_dir_all(&data).unwrap();

        let outside = root.path().join("external-mcp-config.json");
        fs::write(&outside, r#"{"servers":[]}"#).unwrap();
        symlink(&outside, data.join("mcp-servers.json")).unwrap();

        let error = load_servers(&data).unwrap_err();
        assert!(
            error.contains("regular non-symlink file"),
            "symlinked MCP configuration must fail closed: {error}"
        );
    }
}
