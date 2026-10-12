use super::{ToolError, ToolRuntime, atomic_write, ioerr, modified_ns, now_ns, sha256};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const MAX_SNAPSHOT_BYTES: u64 = 12 * 1024 * 1024;
const MAX_SNAPSHOT_SCAN: usize = 5000;

impl ToolRuntime {
    pub(super) fn create_snapshot(&self, session: &str, path: &Path) -> Result<(), ToolError> {
        let existed = path.is_file();
        if path.exists() && !existed {
            return Err(ToolError::new(
                "SNAPSHOT_TARGET_NOT_FILE",
                "snapshots support regular files only",
            ));
        }
        let root = fs::canonicalize(self.workspace_for_session(session)).map_err(ioerr)?;
        let canonical = if existed {
            fs::canonicalize(path).map_err(ioerr)?
        } else {
            let parent = path.parent().ok_or_else(|| {
                ToolError::new("SNAPSHOT_PATH_INVALID", "snapshot target has no parent")
            })?;
            let canonical_parent = fs::canonicalize(parent).map_err(ioerr)?;
            if !canonical_parent.starts_with(&root) {
                return Err(ToolError::new(
                    "PATH_OUTSIDE_WORKSPACE",
                    "snapshot target parent is outside the active workspace",
                ));
            }
            canonical_parent.join(path.file_name().ok_or_else(|| {
                ToolError::new("SNAPSHOT_PATH_INVALID", "snapshot target has no filename")
            })?)
        };
        if !canonical.starts_with(&root) {
            return Err(ToolError::new(
                "PATH_OUTSIDE_WORKSPACE",
                "snapshot target is outside the active session workspace",
            ));
        }
        let relative = canonical
            .strip_prefix(&root)
            .map_err(|_| {
                ToolError::new(
                    "SNAPSHOT_PATH_INVALID",
                    "snapshot target has no workspace-relative path",
                )
            })?
            .to_string_lossy()
            .into_owned();
        if !safe_relative_path(&relative) {
            return Err(ToolError::new(
                "SNAPSHOT_PATH_INVALID",
                "snapshot target path is not safe",
            ));
        }

        let metadata = if existed {
            Some(fs::metadata(&canonical).map_err(ioerr)?)
        } else {
            None
        };
        if metadata
            .as_ref()
            .is_some_and(|m| m.len() > MAX_SNAPSHOT_BYTES)
        {
            return Err(ToolError::new(
                "SNAPSHOT_TOO_LARGE",
                "file exceeds the 12 MiB snapshot safety limit",
            ));
        }
        let data = if existed {
            fs::read(&canonical).map_err(ioerr)?
        } else {
            Vec::new()
        };
        let snapshot_dir = self.config.data_dir.join("snapshots");
        fs::create_dir_all(&snapshot_dir).map_err(ioerr)?;
        let dir_meta = fs::symlink_metadata(&snapshot_dir).map_err(ioerr)?;
        if !dir_meta.is_dir() || dir_meta.file_type().is_symlink() {
            return Err(ToolError::new(
                "SNAPSHOT_DIR_INVALID",
                "snapshot storage must be a real directory",
            ));
        }
        fs::set_permissions(&snapshot_dir, fs::Permissions::from_mode(0o700)).map_err(ioerr)?;

        let id = Uuid::new_v4().simple().to_string();
        let basename = safe_basename(
            canonical
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("file"),
        );
        let data_file = format!("{id}-{basename}");
        let data_path = snapshot_dir.join(&data_file);
        let manifest_path = snapshot_dir.join(format!("{id}.json"));
        atomic_write(&data_path, &data)?;
        let manifest = json!({
            "schema_version": 1,
            "id": id,
            "session_id": session,
            "workspace_root": root.to_string_lossy(),
            "relative_path": relative,
            "existed": existed,
            "data_file": data_file,
            "sha256": sha256(&data),
            "size_bytes": data.len(),
            "created_at_unix_ms": (now_ns() / 1_000_000) as u64,
            "original_modified_ns": metadata.as_ref().map(modified_ns).unwrap_or(0)
        });
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| ToolError::new("SNAPSHOT_METADATA", e.to_string()))?;
        if let Err(error) = atomic_write(&manifest_path, &manifest_bytes) {
            let _ = fs::remove_file(&data_path);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn rewind_command(&self, session: &str, command: &str) -> Result<String, ToolError> {
        let command = command.trim();
        let tail = command.strip_prefix("/rewind").ok_or_else(|| {
            ToolError::new("INVALID_REWIND_COMMAND", "command must start with /rewind")
        })?;
        if !tail.is_empty() && !tail.chars().next().is_some_and(char::is_whitespace) {
            return Err(ToolError::new(
                "INVALID_REWIND_COMMAND",
                "use /rewind, /rewind list, or /rewind <snapshot-id>",
            ));
        }
        let tail = tail.trim();
        if tail.is_empty() || tail == "list" {
            return self.list_rewind_snapshots(session);
        }
        if let Some(rest) = tail.strip_prefix("confirm ") {
            let (id, confirmed_path) = rest.split_once(" --path=").ok_or_else(|| {
                ToolError::new(
                    "REWIND_CONFIRMATION_REQUIRED",
                    "preview the snapshot first, then copy the exact confirmation command shown",
                )
            })?;
            if id.is_empty() || confirmed_path.is_empty() || id.contains(char::is_whitespace) {
                return Err(ToolError::new(
                    "REWIND_CONFIRMATION_REQUIRED",
                    "confirmation must include the snapshot id and exact --path value",
                ));
            }
            return self.restore_rewind_snapshot(session, id, confirmed_path);
        }
        if tail.contains(char::is_whitespace) {
            return Err(ToolError::new(
                "INVALID_REWIND_COMMAND",
                "use /rewind <snapshot-id> to inspect a snapshot before restoring it",
            ));
        }
        let (manifest, _target, data) = self.load_rewind_snapshot(session, tail)?;
        let path = manifest["relative_path"].as_str().unwrap_or("");
        let id = manifest["id"].as_str().unwrap_or("");
        let hash = manifest["sha256"].as_str().unwrap_or("");
        let saved = manifest["created_at_unix_ms"].as_u64().unwrap_or(0);
        let original_state = if manifest["existed"].as_bool() == Some(true) {
            format!("arquivo existente ({} bytes)", data.len())
        } else {
            "arquivo ainda não existia; a restauração removerá o arquivo criado posteriormente"
                .to_owned()
        };
        Ok(format!(
            "Snapshot verificado; nenhum arquivo foi alterado.\nID: {id}\nArquivo exato: {path}\nEstado salvo: {original_state}\nSalvo (Unix ms): {saved}\nTamanho dos dados preservados: {} bytes\nSHA-256 verificado: {hash}\n\nPara restaurar, envie a confirmação explícita:\n/rewind confirm {id} --path={path}",
            data.len()
        ))
    }

    fn list_rewind_snapshots(&self, session: &str) -> Result<String, ToolError> {
        let snapshot_dir = self.config.data_dir.join("snapshots");
        let dir_meta = match fs::symlink_metadata(&snapshot_dir) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok("Ainda não há snapshots seguros para esta sessão. Eles serão criados automaticamente antes das próximas alterações de arquivo.".into());
            }
            Err(error) => return Err(ioerr(error)),
        };
        if !dir_meta.is_dir() || dir_meta.file_type().is_symlink() {
            return Err(ToolError::new(
                "SNAPSHOT_DIR_INVALID",
                "snapshot storage must be a real directory",
            ));
        }

        let mut sidecars = Vec::<(u64, String)>::new();
        for entry in fs::read_dir(&snapshot_dir)
            .map_err(ioerr)?
            .take(MAX_SNAPSHOT_SCAN)
        {
            let entry = entry.map_err(ioerr)?;
            let file_type = entry.file_type().map_err(ioerr)?;
            if !file_type.is_file()
                || file_type.is_symlink()
                || entry.path().extension().and_then(|v| v.to_str()) != Some("json")
            {
                continue;
            }
            let entry_path = entry.path();
            let Some(id) = entry_path.file_stem().and_then(|v| v.to_str()) else {
                continue;
            };
            if !valid_snapshot_id(id) {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
                .unwrap_or(0);
            sidecars.push((modified, id.to_owned()));
        }
        sidecars.sort_by(|a, b| b.0.cmp(&a.0));
        let mut snapshots = Vec::new();
        for (_, id) in sidecars {
            if let Ok((manifest, _, _)) = self.load_rewind_manifest(session, &id) {
                snapshots.push(manifest);
                if snapshots.len() >= 20 {
                    break;
                }
            }
        }
        if snapshots.is_empty() {
            return Ok("Nenhum snapshot com manifesto íntegro pertence a esta sessão. Snapshots legados sem manifesto de sessão/caminho/hash não são restaurados por segurança.".into());
        }
        let mut output = String::from(
            "Snapshots desta sessão (o hash completo será validado novamente ao abrir cada item):\n",
        );
        for item in &snapshots {
            output.push_str(&format!(
                "- ID: {} | arquivo: {} | salvo Unix ms: {} | bytes: {} | SHA-256: {}\n",
                item["id"].as_str().unwrap_or(""),
                item["relative_path"].as_str().unwrap_or(""),
                item["created_at_unix_ms"].as_u64().unwrap_or(0),
                item["size_bytes"].as_u64().unwrap_or(0),
                item["sha256"].as_str().unwrap_or("")
            ));
        }
        output.push_str("\nPara conferir um snapshot sem alterar arquivos: /rewind <snapshot-id>. A restauração exige um segundo comando explícito com o caminho exato.");
        Ok(output)
    }

    fn load_rewind_manifest(
        &self,
        session: &str,
        id: &str,
    ) -> Result<(Value, PathBuf, PathBuf), ToolError> {
        if !valid_snapshot_id(id) {
            return Err(ToolError::new(
                "INVALID_SNAPSHOT_ID",
                "snapshot id must be 32 lowercase hexadecimal characters",
            ));
        }
        let snapshot_dir = self.config.data_dir.join("snapshots");
        let dir_meta = fs::symlink_metadata(&snapshot_dir).map_err(|_| {
            ToolError::new(
                "SNAPSHOT_NOT_FOUND",
                "snapshot does not exist in this session",
            )
        })?;
        if !dir_meta.is_dir() || dir_meta.file_type().is_symlink() {
            return Err(ToolError::new(
                "SNAPSHOT_DIR_INVALID",
                "snapshot storage must be a real directory",
            ));
        }
        let snapshot_root = fs::canonicalize(&snapshot_dir).map_err(ioerr)?;
        let manifest_path = snapshot_root.join(format!("{id}.json"));
        let manifest_meta = fs::symlink_metadata(&manifest_path).map_err(|_| {
            ToolError::new(
                "SNAPSHOT_NOT_FOUND",
                "snapshot does not exist in this session",
            )
        })?;
        if !manifest_meta.is_file() || manifest_meta.file_type().is_symlink() {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot manifest is not a regular file",
            ));
        }
        let canonical_manifest = fs::canonicalize(&manifest_path).map_err(ioerr)?;
        if !canonical_manifest.starts_with(&snapshot_root) {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot manifest escapes snapshot storage",
            ));
        }
        let manifest: Value = serde_json::from_slice(
            &fs::read(&canonical_manifest).map_err(ioerr)?,
        )
        .map_err(|_| ToolError::new("SNAPSHOT_INVALID", "snapshot manifest JSON is invalid"))?;
        if manifest["schema_version"].as_u64() != Some(1) || manifest["id"].as_str() != Some(id) {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot manifest version or id is invalid",
            ));
        }
        if manifest["session_id"].as_str() != Some(session) {
            return Err(ToolError::new(
                "SNAPSHOT_NOT_FOUND",
                "snapshot does not exist in this session",
            ));
        }
        let active_root = fs::canonicalize(self.workspace_for_session(session)).map_err(ioerr)?;
        if manifest["workspace_root"].as_str() != Some(active_root.to_string_lossy().as_ref()) {
            return Err(ToolError::new(
                "SNAPSHOT_WORKSPACE_MISMATCH",
                "snapshot belongs to a different workspace root",
            ));
        }
        let relative = manifest["relative_path"].as_str().ok_or_else(|| {
            ToolError::new("SNAPSHOT_INVALID", "snapshot path metadata is missing")
        })?;
        if !safe_relative_path(relative) {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot path metadata is unsafe",
            ));
        }
        if has_symlink_component(&active_root, relative)? {
            return Err(ToolError::new(
                "SNAPSHOT_PATH_CHANGED",
                "snapshot path now contains a symlink; refusing to restore to a different target",
            ));
        }
        let target = self.resolve_path(relative, true, session)?;
        let expected_target = active_root.join(relative);
        if target.exists() && fs::canonicalize(&expected_target).map_err(ioerr)? != target {
            return Err(ToolError::new(
                "SNAPSHOT_PATH_CHANGED",
                "snapshot target now resolves through a different path; refusing to restore",
            ));
        }
        if target.is_dir() {
            return Err(ToolError::new(
                "SNAPSHOT_TARGET_NOT_FILE",
                "snapshot target is a directory",
            ));
        }
        let basename = safe_basename(
            Path::new(relative)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("file"),
        );
        let expected_data_file = format!("{id}-{basename}");
        if manifest["data_file"].as_str() != Some(expected_data_file.as_str()) {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot data filename does not match its id and target",
            ));
        }
        let data_path = snapshot_root.join(&expected_data_file);
        let data_meta = fs::symlink_metadata(&data_path)
            .map_err(|_| ToolError::new("SNAPSHOT_INVALID", "snapshot content file is missing"))?;
        if !data_meta.is_file() || data_meta.file_type().is_symlink() {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot content is not a regular file",
            ));
        }
        let canonical_data = fs::canonicalize(&data_path).map_err(ioerr)?;
        if canonical_data.parent() != Some(snapshot_root.as_path()) {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot content escapes snapshot storage",
            ));
        }
        if manifest["existed"].as_bool().is_none()
            || (manifest["existed"].as_bool() == Some(false)
                && (data_meta.len() != 0
                    || manifest["sha256"].as_str() != Some(sha256(&[]).as_str())))
        {
            return Err(ToolError::new(
                "SNAPSHOT_INVALID",
                "snapshot existence metadata is inconsistent",
            ));
        }
        if data_meta.len() > MAX_SNAPSHOT_BYTES
            || manifest["size_bytes"].as_u64() != Some(data_meta.len())
        {
            return Err(ToolError::new(
                "SNAPSHOT_SIZE_MISMATCH",
                "snapshot size does not match its manifest or exceeds 12 MiB",
            ));
        }
        Ok((manifest, target, canonical_data))
    }

    fn load_rewind_snapshot(
        &self,
        session: &str,
        id: &str,
    ) -> Result<(Value, PathBuf, Vec<u8>), ToolError> {
        let (manifest, target, data_path) = self.load_rewind_manifest(session, id)?;
        let data = fs::read(&data_path).map_err(ioerr)?;
        let actual = sha256(&data);
        if manifest["sha256"].as_str() != Some(actual.as_str()) {
            return Err(ToolError::new(
                "SNAPSHOT_HASH_MISMATCH",
                "snapshot content hash does not match its manifest; nothing was restored",
            ));
        }
        Ok((manifest, target, data))
    }

    fn restore_rewind_snapshot(
        &self,
        session: &str,
        id: &str,
        confirmed_path: &str,
    ) -> Result<String, ToolError> {
        let (manifest, target, data) = self.load_rewind_snapshot(session, id)?;
        let relative = manifest["relative_path"].as_str().unwrap_or("");
        if confirmed_path != relative {
            return Err(ToolError::new(
                "REWIND_CONFIRMATION_MISMATCH",
                format!("the confirmed path must exactly match {relative:?}; no file was changed"),
            ));
        }
        let parent = target.parent().ok_or_else(|| {
            ToolError::new(
                "SNAPSHOT_TARGET_INVALID",
                "snapshot target has no parent directory",
            )
        })?;
        if !parent.is_dir() {
            return Err(ToolError::new(
                "SNAPSHOT_PARENT_MISSING",
                "target parent directory no longer exists; rewind will not recreate directory trees implicitly",
            ));
        }
        let root = fs::canonicalize(self.workspace_for_session(session)).map_err(ioerr)?;
        let canonical_parent = fs::canonicalize(parent).map_err(ioerr)?;
        if !canonical_parent.starts_with(&root) {
            return Err(ToolError::new(
                "PATH_OUTSIDE_WORKSPACE",
                "restore parent resolves outside the active workspace",
            ));
        }
        let expected_existed = manifest["existed"].as_bool().ok_or_else(|| {
            ToolError::new("SNAPSHOT_INVALID", "snapshot existence metadata is missing")
        })?;
        if target.exists() && !target.is_file() {
            return Err(ToolError::new(
                "SNAPSHOT_TARGET_NOT_FILE",
                "restore target is not a regular file",
            ));
        }
        // Preserve the state being replaced, including an absent-file tombstone,
        // so the rewind itself can be reversed by a subsequent rewind.
        self.create_snapshot(session, &target)?;
        if expected_existed {
            atomic_write(&target, &data)?;
        } else if target.exists() {
            fs::remove_file(&target).map_err(ioerr)?;
        }
        self.reads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(session.to_owned(), target.to_string_lossy().to_string()));
        let outcome = if expected_existed {
            format!(
                "conteúdo restaurado: {} bytes, SHA-256 {}",
                data.len(),
                sha256(&data)
            )
        } else {
            "arquivo removido: ele não existia no momento do snapshot".to_owned()
        };
        Ok(format!(
            "Restauração concluída.\nArquivo: {relative}\nSnapshot restaurado: {id}\nEstado: {outcome}\nO estado anterior foi preservado em um novo snapshot quando o arquivo existia. Faça Read antes de editar novamente.",
        ))
    }
}

fn valid_snapshot_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 2048
        && !value.chars().any(char::is_control)
        && !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn has_symlink_component(root: &Path, relative: &str) -> Result<bool, ToolError> {
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        let Component::Normal(name) = component else {
            return Ok(true);
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ioerr(error)),
        }
    }
    Ok(false)
}

fn safe_basename(value: &str) -> String {
    let name = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    if name.is_empty() {
        "file".into()
    } else {
        name.chars().take(200).collect()
    }
}
