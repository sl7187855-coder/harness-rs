//! Configurable Language Server Protocol client with bounded JSON-RPC framing.
//! Language servers are trusted local executables only; they run in the read-only
//! Landlock/seccomp sandbox with the network blocked.
use crate::sandbox::{PreparedSandbox, Profile};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const LSP_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone, Debug, Deserialize)]
struct LspServerConfig {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}
fn enabled_by_default() -> bool {
    true
}
#[derive(Default, Deserialize)]
struct LspConfigFile {
    #[serde(default)]
    servers: Vec<LspServerConfig>,
}

fn load_servers(data_dir: &Path) -> Result<Vec<LspServerConfig>, String> {
    let path = data_dir.join("lsp-servers.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot inspect LSP config: {error}")),
    };
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > MAX_CONFIG_BYTES {
        return Err(
            "lsp-servers.json must be a regular non-symlink file no larger than 64 KiB".into(),
        );
    }
    let text =
        fs::read_to_string(&path).map_err(|error| format!("cannot read LSP config: {error}"))?;
    let parsed: LspConfigFile =
        serde_json::from_str(&text).map_err(|error| format!("invalid LSP config: {error}"))?;
    if parsed.servers.len() > 20 {
        return Err("LSP config is limited to 20 server entries".into());
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
                "LSP server names must be 1–96 ASCII letters/digits/dot/underscore/hyphen".into(),
            );
        }
        if !names.insert(server.name.clone()) {
            return Err(format!("duplicate LSP server: {}", server.name));
        }
        if server.args.len() > 32 || server.args.iter().any(|arg| arg.len() > 2048) {
            return Err(format!("LSP args exceed limits for {}", server.name));
        }
        if !server.command.starts_with('/') {
            return Err(format!(
                "LSP command for {} must be an absolute executable path",
                server.name
            ));
        }
        let command = fs::canonicalize(&server.command)
            .map_err(|_| format!("LSP executable is unavailable for {}", server.name))?;
        let meta = fs::metadata(&command)
            .map_err(|_| format!("LSP executable is unavailable for {}", server.name))?;
        if !meta.is_file() {
            return Err(format!(
                "LSP command is not a regular file for {}",
                server.name
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                return Err(format!("LSP command is not executable for {}", server.name));
            }
        }
        if server.extensions.is_empty()
            || server.extensions.len() > 40
            || server.extensions.iter().any(|ext| {
                ext.is_empty() || ext.len() > 20 || ext.contains('/') || ext.contains('\\')
            })
        {
            return Err(format!(
                "LSP extensions for {} must be a bounded list of file extensions",
                server.name
            ));
        }
        server.extensions = server
            .extensions
            .into_iter()
            .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
            .collect();
        server.command = command.to_string_lossy().to_string();
        if server.enabled {
            enabled.push(server);
        }
    }
    Ok(enabled)
}

fn language_id(extension: &str) -> &'static str {
    match extension {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "py" => "python",
        "go" => "go",
        "c" => "c",
        "h" => "c",
        "cc" | "cpp" | "cxx" => "cpp",
        "hpp" | "hh" => "cpp",
        "java" => "java",
        "json" => "json",
        "html" => "html",
        "css" => "css",
        "vue" => "vue",
        "lua" => "lua",
        "sh" => "shellscript",
        _ => "plaintext",
    }
}
fn uri_for_file(path: &Path) -> Result<String, String> {
    reqwest::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| "cannot encode workspace file URI".into())
}
fn safe_path(workspace: &Path, raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() || raw.len() > 2000 {
        return Err("LSP file path must contain 1–2000 bytes".into());
    }
    let root = fs::canonicalize(workspace).map_err(|_| "workspace is unavailable".to_owned())?;
    let input = PathBuf::from(raw);
    let candidate = if input.is_absolute() {
        input
    } else {
        root.join(input)
    };
    let path = fs::canonicalize(candidate)
        .map_err(|_| "LSP file does not exist in the workspace".to_owned())?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("LSP file must be a regular file inside the configured workspace".into());
    }
    let relative = path.strip_prefix(&root).unwrap_or(&path);
    let filename = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if relative
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        || [
            ".env",
            "providers.json",
            "credentials.json",
            "id_rsa",
            "id_ed25519",
        ]
        .contains(&filename.as_str())
        || filename.ends_with(".db")
        || filename.ends_with(".db-wal")
        || filename.ends_with(".db-shm")
    {
        return Err("LSP refuses hidden and private files".into());
    }
    let meta = fs::metadata(&path).map_err(|_| "cannot stat LSP file".to_owned())?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "LSP refuses files larger than {MAX_FILE_BYTES} bytes"
        ));
    }
    Ok(path)
}

struct LspClient {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    _sandbox: std::sync::Arc<PreparedSandbox>,
    notifications: Vec<Value>,
    next_id: u64,
}
impl Drop for LspClient {
    fn drop(&mut self) {
        self.child.start_kill().ok();
    }
}
impl LspClient {
    async fn start(server: &LspServerConfig, workspace: &Path) -> Result<Self, String> {
        let sandbox = PreparedSandbox::prepare(workspace, Profile::ReadOnly).map_err(|error| {
            format!("LSP sandbox unavailable; refusing unsandboxed language server: {error}")
        })?;
        let mut command = Command::new(&server.command);
        command
            .args(&server.args)
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
            .map_err(|error| format!("cannot start LSP server {}: {error}", server.name))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "LSP stdin unavailable".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "LSP stdout unavailable".to_owned())?;
        let mut client = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            _sandbox: sandbox,
            notifications: Vec::new(),
            next_id: 1,
        };
        client.initialize(workspace).await?;
        Ok(client)
    }

    async fn send(&mut self, method: &str, params: Value, id: Option<u64>) -> Result<(), String> {
        let mut message = json!({"jsonrpc":"2.0","method":method,"params":params});
        if let Some(id) = id {
            message["id"] = json!(id);
        }
        let body =
            serde_json::to_vec(&message).map_err(|_| "cannot encode LSP message".to_owned())?;
        if body.len() > MAX_MESSAGE_BYTES {
            return Err("LSP request exceeds 1 MiB".into());
        }
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin
            .write_all(header.as_bytes())
            .await
            .map_err(|_| "cannot write LSP header".to_owned())?;
        self.stdin
            .write_all(&body)
            .await
            .map_err(|_| "cannot write LSP body".to_owned())?;
        self.stdin
            .flush()
            .await
            .map_err(|_| "cannot flush LSP message".to_owned())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.send(method, params, Some(id)).await?;
        self.response(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(method, params, None).await
    }

    async fn read_message(&mut self) -> Result<Value, String> {
        let mut content_length = None;
        let mut header_bytes = 0usize;
        loop {
            let mut line = String::new();
            timeout(LSP_TIMEOUT, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| "LSP response header timed out".to_owned())?
                .map_err(|_| "LSP response header failed".to_owned())?;
            if line.is_empty() {
                return Err("LSP server closed stdout".into());
            }
            header_bytes = header_bytes.saturating_add(line.len());
            if header_bytes > 16 * 1024 {
                return Err("LSP response headers exceed 16 KiB".into());
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed
                .split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
            {
                let length = value
                    .1
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid LSP Content-Length".to_owned())?;
                if length > MAX_MESSAGE_BYTES {
                    return Err("LSP response exceeds 1 MiB".into());
                }
                content_length = Some(length);
            }
        }
        let length =
            content_length.ok_or_else(|| "LSP response has no Content-Length header".to_owned())?;
        let mut body = vec![0u8; length];
        timeout(LSP_TIMEOUT, self.stdout.read_exact(&mut body))
            .await
            .map_err(|_| "LSP response body timed out".to_owned())?
            .map_err(|_| "LSP response body truncated".to_owned())?;
        serde_json::from_slice(&body).map_err(|_| "LSP server sent invalid JSON".into())
    }

    async fn response(&mut self, id: u64) -> Result<Value, String> {
        for _ in 0..128 {
            let message = self.read_message().await?;
            if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
                return Err("invalid LSP JSON-RPC version".into());
            }
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(format!(
                        "LSP error {}: {}",
                        error.get("code").and_then(Value::as_i64).unwrap_or(-1),
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("request failed")
                            .chars()
                            .take(300)
                            .collect::<String>()
                    ));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            if message.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.notifications.push(message);
                if self.notifications.len() > 200 {
                    self.notifications.remove(0);
                }
            }
        }
        Err("LSP server exceeded the 128-message response limit".into())
    }

    async fn initialize(&mut self, workspace: &Path) -> Result<(), String> {
        let root_uri = uri_for_file(
            &fs::canonicalize(workspace).map_err(|_| "workspace unavailable".to_owned())?,
        )?;
        let _result = self
            .request(
                "initialize",
                json!({
                    "processId":std::process::id(),
                    "clientInfo":{"name":"harness-rs","version":env!("CARGO_PKG_VERSION")},
                    "rootUri":root_uri,
                    "workspaceFolders":[{"uri":root_uri,"name":"workspace"}],
                    "capabilities":{
                        "textDocument":{
                            "definition":{"dynamicRegistration":false},
                            "references":{"dynamicRegistration":false},
                            "publishDiagnostics":{"relatedInformation":true}
                        },
                        "workspace":{"workspaceFolders":true}
                    }
                }),
            )
            .await?;
        self.notify("initialized", json!({})).await?;
        Ok(())
    }

    async fn open_document(&mut self, path: &Path) -> Result<String, String> {
        let uri = uri_for_file(path)?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let text = fs::read_to_string(path)
            .map_err(|_| "LSP currently accepts UTF-8 text files only".to_owned())?;
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument":{
                "uri":uri,"languageId":language_id(&extension),"version":1,"text":text
            }}),
        )
        .await?;
        Ok(uri)
    }

    async fn request_position(
        &mut self,
        operation: &str,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, String> {
        let params =
            json!({"textDocument":{"uri":uri},"position":{"line":line,"character":character}});
        let method = match operation {
            "definition" => "textDocument/definition",
            "references" => "textDocument/references",
            "diagnostics" => "textDocument/diagnostic",
            _ => return Err("LSP operation must be definition, references or diagnostics".into()),
        };
        let params = if operation == "references" {
            json!({"textDocument":{"uri":uri},"position":{"line":line,"character":character},"context":{"includeDeclaration":true}})
        } else if operation == "diagnostics" {
            json!({"textDocument":{"uri":uri},"previousResultId":null})
        } else {
            params
        };
        match self.request(method, params).await {
            Ok(result) => {
                let notifications = std::mem::take(&mut self.notifications);
                Ok(json!({"result":result,"notifications":notifications}))
            }
            Err(error) if operation == "diagnostics" => {
                let notifications = std::mem::take(&mut self.notifications);
                if notifications.is_empty() {
                    Err(error)
                } else {
                    Ok(
                        json!({"result":{"kind":"full","items":[]},"notifications":notifications,"note":"Pull diagnostics were unsupported; publishDiagnostics notifications were collected."}),
                    )
                }
            }
            Err(error) => Err(error),
        }
    }
}

pub async fn execute(
    data_dir: &Path,
    workspace: &Path,
    operation: &str,
    raw_file: &str,
    line: u32,
    character: u32,
) -> Result<Value, String> {
    if !["definition", "references", "diagnostics"].contains(&operation) {
        return Err("LSP operation must be definition, references or diagnostics".into());
    }
    let path = safe_path(workspace, raw_file)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let servers = load_servers(data_dir)?;
    let server=servers.into_iter().find(|server|server.extensions.iter().any(|candidate|candidate==&extension))
        .ok_or_else(||format!("No enabled LSP server is configured for '.{extension}'. Add one in lsp-servers.json; this host does not assume a language server is installed."))?;
    let mut client = LspClient::start(&server, workspace).await?;
    let uri = client.open_document(&path).await?;
    let result = client
        .request_position(
            operation,
            &uri,
            line.min(1_000_000),
            character.min(1_000_000),
        )
        .await?;
    let encoded = serde_json::to_string(&result).unwrap_or_default();
    if encoded.len() > MAX_OUTPUT_BYTES {
        return Ok(
            json!({"ok":true,"server":server.name,"operation":operation,"file":path.strip_prefix(workspace).unwrap_or(&path).to_string_lossy(),"truncated":true,"preview":encoded.chars().take(MAX_OUTPUT_BYTES/2).collect::<String>()}),
        );
    }
    Ok(
        json!({"ok":true,"server":server.name,"operation":operation,"file":path.strip_prefix(workspace).unwrap_or(&path).to_string_lossy(),"line":line,"character":character,"result":result}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_config_is_empty_by_default_and_rejects_unsafe_commands() {
        let temp = tempfile::tempdir().unwrap();
        assert!(load_servers(temp.path()).unwrap().is_empty());
        fs::write(
            temp.path().join("lsp-servers.json"),
            json!({"servers":[{"name":"rust","command":"rust-analyzer","extensions":["rs"]}]})
                .to_string(),
        )
        .unwrap();
        assert!(load_servers(temp.path()).unwrap_err().contains("absolute"));
    }

    #[tokio::test]
    async fn mock_language_server_definition_and_diagnostics_roundtrip() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        let file = workspace.join("lib.rs");
        fs::write(&file, "fn target() {}\nfn caller() { target(); }\n").unwrap();
        let script = workspace.join("mock_lsp.py");
        fs::write(&script,r#"import json,sys
def read_message():
    length=0
    while True:
        line=sys.stdin.buffer.readline()
        if not line: return None
        if line in (b"\r\n",b"\n"): break
        if line.lower().startswith(b"content-length:"): length=int(line.split(b":",1)[1].strip())
    return json.loads(sys.stdin.buffer.read(length))
def write_message(value):
    body=json.dumps(value,separators=(",",":")).encode()
    sys.stdout.buffer.write(("Content-Length: %d\r\n\r\n"%len(body)).encode()+body)
    sys.stdout.buffer.flush()
while True:
    msg=read_message()
    if msg is None: break
    method=msg.get("method",""); ident=msg.get("id")
    if method=="initialize":
        write_message({"jsonrpc":"2.0","id":ident,"result":{"capabilities":{"definitionProvider":True,"referencesProvider":True,"diagnosticProvider":{"interFileDependencies":False,"workspaceDiagnostics":False}}}})
    elif method=="textDocument/definition":
        write_message({"jsonrpc":"2.0","id":ident,"result":{"uri":msg["params"]["textDocument"]["uri"],"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":9}}}})
    elif method=="textDocument/diagnostic":
        write_message({"jsonrpc":"2.0","id":ident,"result":{"kind":"full","items":[{"severity":1,"message":"MOCK-LSP-DIAGNOSTIC","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}}}]}})
    elif ident is not None: write_message({"jsonrpc":"2.0","id":ident,"result":None})
"#).unwrap();
        fs::write(data.join("lsp-servers.json"),json!({"servers":[{"name":"mock","command":"/usr/bin/python3","args":[script.to_string_lossy()],"extensions":["rs"]}]}).to_string()).unwrap();
        let definition = execute(&data, &workspace, "definition", "lib.rs", 0, 3)
            .await
            .unwrap();
        assert_eq!(
            definition["result"]["result"]["range"]["start"]["character"],
            3
        );
        let diagnostics = execute(&data, &workspace, "diagnostics", "lib.rs", 0, 0)
            .await
            .unwrap();
        assert_eq!(
            diagnostics["result"]["result"]["items"][0]["message"],
            "MOCK-LSP-DIAGNOSTIC"
        );
    }

    #[test]
    fn lsp_safe_path_rejects_symlink_escape_and_private_files() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        fs::write(workspace.join(".env"), "private=test").unwrap();
        let outside = root.path().join("outside.rs");
        fs::write(&outside, "fn outside_secret() {}").unwrap();
        symlink(&outside, workspace.join("leak.rs")).unwrap();

        let private = safe_path(&workspace, ".env").unwrap_err();
        assert!(
            private.contains("hidden and private"),
            "hidden files must be refused: {private}"
        );

        let escaped = safe_path(&workspace, "leak.rs").unwrap_err();
        assert!(
            escaped.contains("inside the configured workspace"),
            "symlink targets outside the workspace must be refused: {escaped}"
        );
    }
}
