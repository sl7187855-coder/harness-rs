use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const LOG_NAME: &str = "audit.log";
const INITIAL_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const MAX_LOG_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct AuditLog {
    directory: Arc<PathBuf>,
    state: Arc<Mutex<AuditState>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditStatus {
    pub healthy: bool,
    pub records: u64,
    pub last_hash: String,
    pub error_code: Option<&'static str>,
}

#[derive(Clone)]
struct AuditState {
    sequence: u64,
    last_hash: String,
    error_code: Option<&'static str>,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            sequence: 0,
            last_hash: INITIAL_HASH.to_owned(),
            error_code: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsignedRecord {
    sequence: u64,
    timestamp_unix_ms: u64,
    event_id: String,
    session_id: String,
    run_id: String,
    tool: String,
    phase: String,
    outcome: String,
    error_code: Option<String>,
    previous_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRecord {
    sequence: u64,
    timestamp_unix_ms: u64,
    event_id: String,
    session_id: String,
    run_id: String,
    tool: String,
    phase: String,
    outcome: String,
    error_code: Option<String>,
    previous_hash: String,
    hash: String,
}

impl AuditRecord {
    fn unsigned(&self) -> UnsignedRecord {
        UnsignedRecord {
            sequence: self.sequence,
            timestamp_unix_ms: self.timestamp_unix_ms,
            event_id: self.event_id.clone(),
            session_id: self.session_id.clone(),
            run_id: self.run_id.clone(),
            tool: self.tool.clone(),
            phase: self.phase.clone(),
            outcome: self.outcome.clone(),
            error_code: self.error_code.clone(),
            previous_hash: self.previous_hash.clone(),
        }
    }
}

impl AuditLog {
    /// Open the append-only chain. Startup checks the existing chain before any
    /// event can be appended; invalid storage remains unhealthy and fail-closed.
    pub fn new(data_dir: &Path) -> Self {
        let directory = data_dir.to_path_buf();
        let state = match load_chain(&directory) {
            Ok(state) => state,
            Err(code) => AuditState {
                error_code: Some(code),
                ..AuditState::empty()
            },
        };
        Self {
            directory: Arc::new(directory),
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn status(&self) -> AuditStatus {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        AuditStatus {
            healthy: state.error_code.is_none(),
            records: state.sequence,
            last_hash: state.last_hash.clone(),
            error_code: state.error_code,
        }
    }

    /// Append one bounded metadata-only event and force it to stable storage.
    /// Arguments, tool results, prompts, provider data, and credentials are
    /// intentionally never written to the audit log.
    pub fn append(
        &self,
        session_id: &str,
        run_id: &str,
        tool: &str,
        phase: &str,
        outcome: &str,
        error_code: Option<&str>,
    ) -> Result<(), &'static str> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(code) = state.error_code {
            return Err(code);
        }
        if !matches!(phase, "start" | "finish")
            || !matches!(
                outcome,
                "started" | "completed" | "failed" | "cancelled" | "denied"
            )
        {
            return Err("AUDIT_FIELD_INVALID");
        }
        let session_id = bounded_identifier(session_id, 128)?;
        let run_id = bounded_identifier(run_id, 128)?;
        let tool = bounded_identifier(tool, 96)?;
        let error_code = error_code.map(safe_error_code);

        if let Err(code) = secure_directory(&self.directory) {
            state.error_code = Some(code);
            return Err(code);
        }
        let path = self.directory.join(LOG_NAME);
        let existing_size = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => metadata.len(),
            Ok(_) => {
                state.error_code = Some("AUDIT_PATH_INVALID");
                return Err("AUDIT_PATH_INVALID");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(_) => {
                state.error_code = Some("AUDIT_STORAGE_UNAVAILABLE");
                return Err("AUDIT_STORAGE_UNAVAILABLE");
            }
        };
        if existing_size > MAX_LOG_BYTES {
            state.error_code = Some("AUDIT_LOG_LIMIT");
            return Err("AUDIT_LOG_LIMIT");
        }
        let sequence = state
            .sequence
            .checked_add(1)
            .ok_or("AUDIT_SEQUENCE_EXHAUSTED")?;
        let timestamp_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let unsigned = UnsignedRecord {
            sequence,
            timestamp_unix_ms,
            event_id: Uuid::new_v4().simple().to_string(),
            session_id,
            run_id,
            tool,
            phase: phase.to_owned(),
            outcome: outcome.to_owned(),
            error_code,
            previous_hash: state.last_hash.clone(),
        };
        let hash = match hash_record(&unsigned) {
            Ok(hash) => hash,
            Err(code) => {
                state.error_code = Some(code);
                return Err(code);
            }
        };
        let record = AuditRecord {
            sequence: unsigned.sequence,
            timestamp_unix_ms: unsigned.timestamp_unix_ms,
            event_id: unsigned.event_id,
            session_id: unsigned.session_id,
            run_id: unsigned.run_id,
            tool: unsigned.tool,
            phase: unsigned.phase,
            outcome: unsigned.outcome,
            error_code: unsigned.error_code,
            previous_hash: unsigned.previous_hash,
            hash: hash.clone(),
        };
        let mut bytes = match serde_json::to_vec(&record) {
            Ok(bytes) => bytes,
            Err(_) => {
                state.error_code = Some("AUDIT_SERIALIZE_FAILED");
                return Err("AUDIT_SERIALIZE_FAILED");
            }
        };
        if bytes.len() + 1 > MAX_RECORD_BYTES {
            state.error_code = Some("AUDIT_RECORD_TOO_LARGE");
            return Err("AUDIT_RECORD_TOO_LARGE");
        }
        bytes.push(b'\n');
        if existing_size.saturating_add(bytes.len() as u64) > MAX_LOG_BYTES {
            state.error_code = Some("AUDIT_LOG_LIMIT");
            return Err("AUDIT_LOG_LIMIT");
        }

        let result = (|| {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .write(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
                .map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
            let metadata = file.metadata().map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
            if !metadata.file_type().is_file() {
                return Err("AUDIT_PATH_INVALID");
            }
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
            file.write_all(&bytes).map_err(|_| "AUDIT_WRITE_FAILED")?;
            file.sync_data().map_err(|_| "AUDIT_WRITE_FAILED")?;
            Ok(())
        })();
        if let Err(code) = result {
            // A failed/partial append cannot be safely resumed in this process.
            // Fail closed rather than continuing from an uncertain chain head.
            state.error_code = Some(code);
            return Err(code);
        }
        state.sequence = sequence;
        state.last_hash = hash;
        Ok(())
    }
}

fn secure_directory(directory: &Path) -> Result<(), &'static str> {
    fs::create_dir_all(directory).map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
    let metadata = fs::symlink_metadata(directory).map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("AUDIT_PATH_INVALID");
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
    Ok(())
}

fn load_chain(directory: &Path) -> Result<AuditState, &'static str> {
    secure_directory(directory)?;
    let path = directory.join(LOG_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => return Err("AUDIT_PATH_INVALID"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AuditState::empty());
        }
        Err(_) => return Err("AUDIT_STORAGE_UNAVAILABLE"),
    };
    if metadata.len() > MAX_LOG_BYTES {
        return Err("AUDIT_LOG_LIMIT");
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
    let mut reader = BufReader::new(file);
    let mut state = AuditState::empty();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|_| "AUDIT_STORAGE_UNAVAILABLE")?;
        if read == 0 {
            break;
        }
        if read > MAX_RECORD_BYTES || !line.ends_with(b"\n") {
            return Err("AUDIT_CHAIN_CORRUPT");
        }
        line.pop();
        let record: AuditRecord =
            serde_json::from_slice(&line).map_err(|_| "AUDIT_CHAIN_CORRUPT")?;
        if record.sequence != state.sequence.saturating_add(1)
            || record.previous_hash != state.last_hash
            || !valid_loaded_record(&record)
        {
            return Err("AUDIT_CHAIN_CORRUPT");
        }
        let expected_hash = hash_record(&record.unsigned())?;
        if record.hash != expected_hash {
            return Err("AUDIT_CHAIN_CORRUPT");
        }
        state.sequence = record.sequence;
        state.last_hash = record.hash;
    }
    Ok(state)
}

fn valid_loaded_record(record: &AuditRecord) -> bool {
    !record.event_id.is_empty()
        && record.event_id.len() <= 64
        && record.session_id.len() <= 128
        && record.run_id.len() <= 128
        && !record.session_id.chars().any(char::is_control)
        && !record.run_id.chars().any(char::is_control)
        && !record.tool.is_empty()
        && record.tool.len() <= 96
        && !record.tool.chars().any(char::is_control)
        && matches!(record.phase.as_str(), "start" | "finish")
        && matches!(
            record.outcome.as_str(),
            "started" | "completed" | "failed" | "cancelled" | "denied"
        )
        && record.error_code.as_deref().is_none_or(valid_error_code)
        && record.hash.len() == 64
}

fn bounded_identifier(value: &str, maximum: usize) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) || value.chars().count() > maximum {
        return Err("AUDIT_FIELD_INVALID");
    }
    Ok(value.to_owned())
}

fn safe_error_code(value: &str) -> String {
    if valid_error_code(value) {
        value.to_owned()
    } else {
        "UNCLASSIFIED".to_owned()
    }
}

fn valid_error_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

fn hash_record(record: &UnsignedRecord) -> Result<String, &'static str> {
    let canonical = serde_json::to_vec(record).map_err(|_| "AUDIT_SERIALIZE_FAILED")?;
    let digest = Sha256::digest(canonical);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn audit_chain_appends_and_reopens_with_the_same_head() {
        let temp = tempfile::tempdir().unwrap();
        let log = AuditLog::new(temp.path());
        assert!(log.status().healthy);
        log.append("session-a", "run-a", "Write", "start", "started", None)
            .unwrap();
        log.append("session-a", "run-a", "Write", "finish", "completed", None)
            .unwrap();
        let first = log.status();
        assert!(first.healthy);
        assert_eq!(first.records, 2);

        let reopened = AuditLog::new(temp.path());
        assert!(reopened.status().healthy);
        assert_eq!(reopened.status().records, 2);
        assert_eq!(reopened.status().last_hash, first.last_hash);
        reopened
            .append("session-b", "run-b", "Bash", "start", "started", None)
            .unwrap();
        assert_eq!(reopened.status().records, 3);
    }

    #[test]
    fn audit_chain_rejects_tampering_and_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let log = AuditLog::new(temp.path());
        log.append("session-a", "run-a", "Edit", "start", "started", None)
            .unwrap();
        let path = temp.path().join(LOG_NAME);
        let raw = fs::read_to_string(&path).unwrap();
        let tampered = raw.replacen("started", "failed", 1);
        assert_ne!(raw, tampered);
        fs::write(&path, tampered).unwrap();

        let reopened = AuditLog::new(temp.path());
        assert!(!reopened.status().healthy);
        assert_eq!(reopened.status().error_code, Some("AUDIT_CHAIN_CORRUPT"));
        assert_eq!(
            reopened
                .append("session-a", "run-a", "Edit", "finish", "completed", None)
                .unwrap_err(),
            "AUDIT_CHAIN_CORRUPT"
        );
    }

    #[test]
    fn audit_chain_serializes_concurrent_appends() {
        let temp = tempfile::tempdir().unwrap();
        let log = AuditLog::new(temp.path());
        let mut threads = Vec::new();
        for worker in 0..8 {
            let log = log.clone();
            threads.push(thread::spawn(move || {
                for iteration in 0..10 {
                    log.append(
                        &format!("session-{worker}"),
                        &format!("run-{iteration}"),
                        "Read",
                        "finish",
                        "completed",
                        None,
                    )
                    .unwrap();
                }
            }));
        }
        for handle in threads {
            handle.join().unwrap();
        }
        assert!(log.status().healthy);
        assert_eq!(log.status().records, 80);
        let reopened = AuditLog::new(temp.path());
        assert!(reopened.status().healthy);
        assert_eq!(reopened.status().records, 80);
    }

    #[cfg(unix)]
    #[test]
    fn audit_log_rejects_a_symlink_instead_of_following_it() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path();
        let outside = temp.path().join("outside.jsonl");
        fs::write(&outside, b"do not append here\n").unwrap();
        symlink(&outside, directory.join(LOG_NAME)).unwrap();

        let log = AuditLog::new(temp.path());
        assert!(!log.status().healthy);
        assert_eq!(log.status().error_code, Some("AUDIT_PATH_INVALID"));
        assert_eq!(
            log.append("session-a", "run-a", "Bash", "start", "started", None)
                .unwrap_err(),
            "AUDIT_PATH_INVALID"
        );
        assert_eq!(fs::read(&outside).unwrap(), b"do not append here\n");
    }
}
