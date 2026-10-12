use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct MemoryStore {
    inner: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub prompt: String,
    pub kind: String,
    pub next_run_at: i64,
    pub interval_seconds: i64,
    pub enabled: bool,
    pub running: bool,
    pub created_at: i64,
    pub last_run_at: i64,
    pub last_result: String,
    pub provider: String,
    pub model: String,
    pub api_id: String,
    pub endpoint: String,
    pub effort: String,
    pub run_started_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub session_id: String,
    pub subject: String,
    pub description: String,
    pub active_form: String,
    pub status: String,
    pub blocked_by: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn json_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
}

/// Conservative credential filter shared by the API and model-callable memory tools.
pub fn is_safe_memory_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let obvious_labels = [
        "api_key",
        "api-key",
        "api key:",
        "password",
        "passwd",
        "bearer ",
        "secret_key",
        "private key",
        "token=",
    ];
    if obvious_labels.iter().any(|needle| lower.contains(needle)) {
        return false;
    }
    const PATTERNS: &[&str] = &[
        r"(?i)\b(?:sk-or-v1|sk|rk)-[a-z0-9_-]{16,}\b",
        r"(?i)\bAKIA[0-9A-Z]{16}\b",
        r"\beyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\b",
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
        r"(?i)\b(?:token|secret|password)\s*[:=]\s*[^\s,;]{12,}",
    ];
    !PATTERNS
        .iter()
        .any(|pattern| regex::Regex::new(pattern).is_ok_and(|re| re.is_match(text)))
}

impl MemoryStore {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -16_384)?;
        conn.pragma_update(None, "mmap_size", 0)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory (
                id INTEGER PRIMARY KEY,
                kind TEXT NOT NULL,
                text TEXT NOT NULL,
                scope TEXT NOT NULL DEFAULT 'global',
                importance REAL NOT NULL DEFAULT 0.5,
                confidence REAL NOT NULL DEFAULT 0.7,
                source TEXT NOT NULL DEFAULT 'user',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_used_at INTEGER NOT NULL DEFAULT 0,
                use_count INTEGER NOT NULL DEFAULT 0,
                superseded_by INTEGER,
                ttl INTEGER
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(text, content='memory', content_rowid='id');
             CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory BEGIN
                INSERT INTO memory_fts(rowid, text) VALUES (new.id, new.text);
             END;
             CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, text) VALUES('delete', old.id, old.text);
             END;
             CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE OF text ON memory BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, text) VALUES('delete', old.id, old.text);
                INSERT INTO memory_fts(rowid, text) VALUES (new.id, new.text);
             END;
             CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS session_messages (
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS session_messages_lookup ON session_messages(session_id, id);
             CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                subject TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                active_form TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                blocked_by TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS tasks_session ON tasks(session_id, created_at);
             CREATE TABLE IF NOT EXISTS scheduled_jobs (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                name TEXT NOT NULL,
                prompt TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('once','interval','wakeup')),
                next_run_at INTEGER NOT NULL,
                interval_seconds INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                running INTEGER NOT NULL DEFAULT 0,
                run_started_at INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                last_run_at INTEGER NOT NULL DEFAULT 0,
                last_result TEXT NOT NULL DEFAULT '',
                provider TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                api_id TEXT NOT NULL DEFAULT '',
                endpoint TEXT NOT NULL DEFAULT '',
                effort TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS scheduled_jobs_due ON scheduled_jobs(enabled, running, next_run_at);
             CREATE INDEX IF NOT EXISTS scheduled_jobs_session ON scheduled_jobs(session_id, created_at);"
        )?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn save_memory(
        &self,
        kind: &str,
        text: &str,
        scope: &str,
        importance: f64,
        confidence: f64,
        source: &str,
    ) -> rusqlite::Result<i64> {
        let t = now();
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("INSERT INTO memory(kind,text,scope,importance,confidence,source,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",
            params![kind, text, scope, importance.clamp(0.0,1.0), confidence.clamp(0.0,1.0), source, t])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn search_memory(
        &self,
        query: &str,
        scope: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<Value>> {
        let words = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .take(12)
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT m.id,m.kind,m.text,m.scope,m.importance,m.confidence,m.source,m.created_at,bm25(memory_fts)
             FROM memory_fts JOIN memory m ON m.id=memory_fts.rowid
             WHERE memory_fts MATCH ?1 AND (m.scope='global' OR m.scope=?2) AND (m.ttl IS NULL OR m.ttl>?3)
             ORDER BY bm25(memory_fts), m.importance DESC, m.updated_at DESC LIMIT ?4"
        )?;
        let rows = stmt.query_map(params![words, scope, now(), limit.clamp(1, 50) as i64], |row| {
            Ok(json!({
                "id":row.get::<_,i64>(0)?,"kind":row.get::<_,String>(1)?,"text":row.get::<_,String>(2)?,
                "scope":row.get::<_,String>(3)?,"importance":row.get::<_,f64>(4)?,"confidence":row.get::<_,f64>(5)?,
                "source":row.get::<_,String>(6)?,"created_at":row.get::<_,i64>(7)?
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let value = row?;
            if let Some(id) = value["id"].as_i64() {
                let _ = conn.execute(
                    "UPDATE memory SET last_used_at=?1,use_count=use_count+1 WHERE id=?2",
                    params![now(), id],
                );
            }
            result.push(value);
        }
        Ok(result)
    }

    pub fn forget_memory(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn.execute("DELETE FROM memory WHERE id=?1", [id])? > 0)
    }

    pub fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> rusqlite::Result<()> {
        let t = now();
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("INSERT INTO sessions(session_id,created_at,updated_at) VALUES(?1,?2,?2) ON CONFLICT(session_id) DO UPDATE SET updated_at=excluded.updated_at",
            params![session_id,t])?;
        conn.execute(
            "INSERT INTO session_messages(session_id,role,content,created_at) VALUES(?1,?2,?3,?4)",
            params![session_id, role, content, t],
        )?;
        Ok(())
    }

    pub fn history(&self, session_id: &str, limit: usize) -> rusqlite::Result<Vec<Value>> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT role,content FROM session_messages WHERE session_id=?1 ORDER BY id DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![session_id, limit.clamp(1, 100) as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (role, content) = row?;
            items.push(json!({"role":role,"content":content}));
        }
        items.reverse();
        Ok(items)
    }

    pub fn create_task(
        &self,
        session_id: &str,
        subject: &str,
        description: &str,
        active_form: &str,
        blocked_by: &[String],
    ) -> rusqlite::Result<TaskRecord> {
        if blocked_by.len() > 32 {
            return Err(rusqlite::Error::InvalidParameterName(
                "a task may depend on at most 32 tasks".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let t = now();
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut dependencies = Vec::new();
        for dep in blocked_by {
            let exists:i64=conn.query_row("SELECT EXISTS(SELECT 1 FROM tasks WHERE session_id=?1 AND id=?2 AND status!='deleted')",params![session_id,dep],|r|r.get(0))?;
            if exists == 0 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "blocked-by task {dep} does not exist in this session"
                )));
            }
            if !dependencies.contains(dep) {
                dependencies.push(dep.clone());
            }
        }
        conn.execute("INSERT INTO tasks(id,session_id,subject,description,active_form,status,blocked_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'pending',?6,?7,?7)",
            params![id,session_id,subject,description,active_form,serde_json::to_string(&dependencies).map_err(json_err)?,t])?;
        Ok(TaskRecord {
            id,
            session_id: session_id.to_owned(),
            subject: subject.to_owned(),
            description: description.to_owned(),
            active_form: active_form.to_owned(),
            status: "pending".into(),
            blocked_by: dependencies,
            created_at: t,
            updated_at: t,
        })
    }

    pub fn get_task(&self, session_id: &str, id: &str) -> rusqlite::Result<Option<TaskRecord>> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row("SELECT id,session_id,subject,description,active_form,status,blocked_by,created_at,updated_at FROM tasks WHERE session_id=?1 AND id=?2",
            params![session_id,id], task_from_row).optional()
    }

    pub fn list_tasks(&self, session_id: &str) -> rusqlite::Result<Vec<TaskRecord>> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT id,session_id,subject,description,active_form,status,blocked_by,created_at,updated_at FROM tasks WHERE session_id=?1 AND status!='deleted' ORDER BY created_at")?;
        let rows = stmt.query_map([session_id], task_from_row)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn update_task(
        &self,
        session_id: &str,
        id: &str,
        status: Option<&str>,
        subject: Option<&str>,
        description: Option<&str>,
    ) -> rusqlite::Result<Option<TaskRecord>> {
        let current = self.get_task(session_id, id)?;
        let Some(task) = current else {
            return Ok(None);
        };
        let new_status = status.unwrap_or(&task.status);
        if !["pending", "in_progress", "completed", "deleted"].contains(&new_status) {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid task status".into(),
            ));
        }
        let new_subject = subject.unwrap_or(&task.subject);
        let new_description = description.unwrap_or(&task.description);
        let t = now();
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("UPDATE tasks SET status=?1,subject=?2,description=?3,updated_at=?4 WHERE session_id=?5 AND id=?6",
            params![new_status,new_subject,new_description,t,session_id,id])?;
        drop(conn);
        self.get_task(session_id, id)
    }

    pub fn update_task_dependencies(
        &self,
        session_id: &str,
        task_id: &str,
        add_blocked_by: &[String],
        add_blocks: &[String],
    ) -> rusqlite::Result<Option<TaskRecord>> {
        if add_blocked_by.len() > 32 || add_blocks.len() > 32 {
            return Err(rusqlite::Error::InvalidParameterName(
                "a single task update may add at most 32 dependency edges".into(),
            ));
        }
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let current:Option<TaskRecord>=conn.query_row(
            "SELECT id,session_id,subject,description,active_form,status,blocked_by,created_at,updated_at FROM tasks WHERE session_id=?1 AND id=?2",
            params![session_id,task_id],task_from_row
        ).optional()?;
        if current.is_none() {
            return Ok(None);
        }
        let mut stmt = conn
            .prepare("SELECT id,blocked_by FROM tasks WHERE session_id=?1 AND status!='deleted'")?;
        let rows = stmt.query_map([session_id], |r| {
            let id: String = r.get(0)?;
            let raw: String = r.get(1)?;
            let deps: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
            Ok((id, deps))
        })?;
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            let (id, deps) = row?;
            graph.insert(id, deps);
        }
        drop(stmt);
        let mut current_deps = graph.get(task_id).cloned().unwrap_or_default();
        for dep in add_blocked_by {
            if !graph.contains_key(dep) {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "blocked-by task {dep} does not exist in this session"
                )));
            }
            if graph_reaches(&graph, dep, task_id) {
                return Err(rusqlite::Error::InvalidParameterName(
                    "dependency would create a cycle".into(),
                ));
            }
            if !current_deps.contains(dep) {
                current_deps.push(dep.clone());
            }
            graph.insert(task_id.to_owned(), current_deps.clone());
        }
        let mut changed_targets: HashMap<String, Vec<String>> = HashMap::new();
        for target in add_blocks {
            if !graph.contains_key(target) {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "target task {target} does not exist in this session"
                )));
            }
            if graph_reaches(&graph, task_id, target) {
                return Err(rusqlite::Error::InvalidParameterName(
                    "dependency would create a cycle".into(),
                ));
            }
            let mut deps = graph.get(target).cloned().unwrap_or_default();
            if !deps.iter().any(|d| d == task_id) {
                deps.push(task_id.to_owned());
            }
            graph.insert(target.clone(), deps.clone());
            changed_targets.insert(target.clone(), deps);
        }
        let timestamp = now();
        conn.execute(
            "UPDATE tasks SET blocked_by=?1,updated_at=?2 WHERE session_id=?3 AND id=?4",
            params![
                serde_json::to_string(&current_deps).map_err(json_err)?,
                timestamp,
                session_id,
                task_id
            ],
        )?;
        for (target, deps) in changed_targets {
            conn.execute(
                "UPDATE tasks SET blocked_by=?1,updated_at=?2 WHERE session_id=?3 AND id=?4",
                params![
                    serde_json::to_string(&deps).map_err(json_err)?,
                    timestamp,
                    session_id,
                    target
                ],
            )?;
        }
        drop(conn);
        self.get_task(session_id, task_id)
    }

    pub fn create_schedule(
        &self,
        session_id: &str,
        name: &str,
        prompt: &str,
        kind: &str,
        next_run_at: i64,
        interval_seconds: i64,
        provider: &str,
        model: &str,
        api_id: &str,
        endpoint: &str,
        effort: &str,
    ) -> rusqlite::Result<ScheduledJob> {
        if !["once", "interval", "wakeup"].contains(&kind) {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid schedule kind".into(),
            ));
        }
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if kind == "wakeup" {
            conn.execute(
                "DELETE FROM scheduled_jobs WHERE session_id=?1 AND kind='wakeup' AND running=0",
                [session_id],
            )?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        conn.execute(
            "INSERT INTO scheduled_jobs(id,session_id,name,prompt,kind,next_run_at,interval_seconds,enabled,running,created_at,provider,model,api_id,endpoint,effort,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,1,0,?8,?9,?10,?11,?12,?13,?8)",
            params![id,session_id,name,prompt,kind,next_run_at,interval_seconds,created_at,provider,model,api_id,endpoint,effort]
        )?;
        conn.query_row("SELECT id,session_id,name,prompt,kind,next_run_at,interval_seconds,enabled,running,created_at,last_run_at,last_result,provider,model,api_id,endpoint,effort,run_started_at FROM scheduled_jobs WHERE id=?1",[id],scheduled_job_from_row)
    }
    pub fn list_schedules(&self, session_id: &str) -> rusqlite::Result<Vec<ScheduledJob>> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt=conn.prepare("SELECT id,session_id,name,prompt,kind,next_run_at,interval_seconds,enabled,running,created_at,last_run_at,last_result,provider,model,api_id,endpoint,effort,run_started_at FROM scheduled_jobs WHERE session_id=?1 ORDER BY created_at DESC LIMIT 100")?;
        let rows = stmt.query_map([session_id], scheduled_job_from_row)?;
        rows.collect()
    }
    pub fn delete_schedule(&self, session_id: &str, id: &str) -> rusqlite::Result<bool> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn.execute(
            "DELETE FROM scheduled_jobs WHERE session_id=?1 AND id=?2 AND running=0",
            params![session_id, id],
        )? > 0)
    }
    pub fn clear_wakeups(&self, session_id: &str) -> rusqlite::Result<usize> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM scheduled_jobs WHERE session_id=?1 AND kind='wakeup' AND running=0",
            [session_id],
        )
    }
    pub fn claim_due_schedules(
        &self,
        now_at: i64,
        limit: usize,
    ) -> rusqlite::Result<Vec<ScheduledJob>> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let stale_before = now_at.saturating_sub(1800);
        let due = {
            let mut stmt=conn.prepare("SELECT id,session_id,name,prompt,kind,next_run_at,interval_seconds,enabled,running,created_at,last_run_at,last_result,provider,model,api_id,endpoint,effort,run_started_at FROM scheduled_jobs WHERE enabled=1 AND next_run_at<=?1 AND (running=0 OR run_started_at<?2) ORDER BY next_run_at LIMIT ?3")?;
            let rows = stmt.query_map(
                params![now_at, stale_before, limit.min(4) as i64],
                scheduled_job_from_row,
            )?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut claimed = Vec::new();
        for mut job in due {
            let changed=conn.execute("UPDATE scheduled_jobs SET running=1,run_started_at=?1,updated_at=?1 WHERE id=?2 AND enabled=1 AND next_run_at<=?1 AND (running=0 OR run_started_at<?3)",params![now_at,job.id,stale_before])?;
            if changed > 0 {
                job.running = true;
                job.run_started_at = now_at;
                claimed.push(job);
            }
        }
        Ok(claimed)
    }
    pub fn finish_schedule(
        &self,
        id: &str,
        finished_at: i64,
        result: &str,
    ) -> rusqlite::Result<bool> {
        let safe_result = result.chars().take(12_000).collect::<String>();
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn.execute(
            "UPDATE scheduled_jobs SET last_run_at=?1,last_result=?2,running=0,run_started_at=0,enabled=CASE WHEN kind='interval' THEN 1 ELSE 0 END,next_run_at=CASE WHEN kind='interval' THEN ?1+interval_seconds ELSE next_run_at END,updated_at=?1 WHERE id=?3",
            params![finished_at,safe_result,id]
        )?>0)
    }

    pub fn list_memories(&self, limit: usize) -> rusqlite::Result<Vec<Value>> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id,kind,text,scope,importance,confidence,source,created_at,updated_at,last_used_at,use_count FROM memory WHERE superseded_by IS NULL AND (ttl IS NULL OR ttl>?1) ORDER BY importance DESC, updated_at DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![now(), limit.clamp(1, 500) as i64], |row| {
            Ok(json!({
                "id":row.get::<_,i64>(0)?,"kind":row.get::<_,String>(1)?,"text":row.get::<_,String>(2)?,
                "scope":row.get::<_,String>(3)?,"importance":row.get::<_,f64>(4)?,"confidence":row.get::<_,f64>(5)?,
                "source":row.get::<_,String>(6)?,"created_at":row.get::<_,i64>(7)?,"updated_at":row.get::<_,i64>(8)?,
                "last_used_at":row.get::<_,i64>(9)?,"use_count":row.get::<_,i64>(10)?
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn counts(&self) -> rusqlite::Result<Value> {
        let conn = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let memories: i64 = conn.query_row("SELECT count(*) FROM memory", [], |r| r.get(0))?;
        let sessions: i64 = conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))?;
        let tasks: i64 = conn.query_row(
            "SELECT count(*) FROM tasks WHERE status!='deleted'",
            [],
            |r| r.get(0),
        )?;
        let schedules: i64 = conn.query_row(
            "SELECT count(*) FROM scheduled_jobs WHERE enabled=1",
            [],
            |r| r.get(0),
        )?;
        Ok(json!({"memories":memories,"sessions":sessions,"tasks":tasks,"schedules":schedules}))
    }
}

fn scheduled_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledJob> {
    Ok(ScheduledJob {
        id: row.get(0)?,
        session_id: row.get(1)?,
        name: row.get(2)?,
        prompt: row.get(3)?,
        kind: row.get(4)?,
        next_run_at: row.get(5)?,
        interval_seconds: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        running: row.get::<_, i64>(8)? != 0,
        created_at: row.get(9)?,
        last_run_at: row.get(10)?,
        last_result: row.get(11)?,
        provider: row.get(12)?,
        model: row.get(13)?,
        api_id: row.get(14)?,
        endpoint: row.get(15)?,
        effort: row.get(16)?,
        run_started_at: row.get(17)?,
    })
}
fn graph_reaches(graph: &HashMap<String, Vec<String>>, start: &str, target: &str) -> bool {
    let mut stack = vec![start.to_owned()];
    let mut visited = HashSet::new();
    while let Some(node) = stack.pop() {
        if node == target {
            return true;
        }
        if !visited.insert(node.clone()) {
            continue;
        }
        if let Some(next) = graph.get(&node) {
            stack.extend(next.iter().cloned());
        }
    }
    false
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    let blocked: String = row.get(6)?;
    Ok(TaskRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        subject: row.get(2)?,
        description: row.get(3)?,
        active_form: row.get(4)?,
        status: row.get(5)?,
        blocked_by: serde_json::from_str(&blocked).unwrap_or_default(),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_secret_filter_rejects_credentials_but_accepts_project_facts() {
        assert!(is_safe_memory_text("The project uses Rust and SQLite."));
        assert!(!is_safe_memory_text(
            "api_key=sk-or-v1-123456789012345678901234"
        ));
        assert!(!is_safe_memory_text(
            "Authorization: Bearer verylongcredentialvalue123456"
        ));
        assert!(!is_safe_memory_text("-----BEGIN PRIVATE KEY-----"));
        assert!(!is_safe_memory_text("token=abcde123456789012345"));
    }
    #[test]
    fn memory_save_search_forget_removes_the_fts_entry() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&temp.path().join("memory.sqlite")).unwrap();
        let id = store
            .save_memory(
                "fact",
                "The project uses Rust with SQLite.",
                "global",
                0.8,
                0.9,
                "user_explicit",
            )
            .unwrap();
        let found = store.search_memory("SQLite", "project:test", 10).unwrap();
        assert!(found.iter().any(|item| item["id"].as_i64() == Some(id)));
        assert!(store.forget_memory(id).unwrap());
        let after = store.search_memory("SQLite", "project:test", 10).unwrap();
        assert!(!after.iter().any(|item| item["id"].as_i64() == Some(id)));
        assert!(!store.forget_memory(id).unwrap());
    }
    #[test]
    fn dependency_edges_are_validated_and_cycles_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&temp.path().join("tasks.sqlite")).unwrap();
        let a = store.create_task("session", "A", "", "", &[]).unwrap();
        let b = store.create_task("session", "B", "", "", &[]).unwrap();
        let c = store.create_task("session", "C", "", "", &[]).unwrap();
        let updated = store
            .update_task_dependencies("session", &a.id, &[b.id.clone()], &[c.id.clone()])
            .unwrap()
            .unwrap();
        assert_eq!(updated.blocked_by, vec![b.id.clone()]);
        assert_eq!(
            store
                .get_task("session", &c.id)
                .unwrap()
                .unwrap()
                .blocked_by,
            vec![a.id.clone()]
        );
        let cycle = store
            .update_task_dependencies("session", &b.id, &[c.id.clone()], &[])
            .unwrap_err();
        assert!(cycle.to_string().contains("cycle"));
        let missing = store
            .update_task_dependencies("session", &b.id, &["missing".to_owned()], &[])
            .unwrap_err();
        assert!(missing.to_string().contains("does not exist"));
        let unknown_create = store
            .create_task("session", "invalid", "", "", &["missing".to_owned()])
            .unwrap_err();
        assert!(unknown_create.to_string().contains("does not exist"));
    }
    #[test]
    fn schedules_are_persistent_claimed_once_and_session_scoped() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&temp.path().join("schedules.sqlite")).unwrap();
        let due_at = now() + 3600;
        let job = store
            .create_schedule(
                "alice",
                "Check workspace",
                "Summarize changed files",
                "interval",
                due_at,
                300,
                "openrouter",
                "model-x",
                "profile-1",
                "https://example.invalid",
                "low",
            )
            .unwrap();
        assert!(store.list_schedules("bob").unwrap().is_empty());
        assert_eq!(store.list_schedules("alice").unwrap().len(), 1);
        let claimed = store.claim_due_schedules(due_at, 2).unwrap();
        assert_eq!(claimed.len(), 1);
        assert!(claimed[0].running);
        assert!(store.claim_due_schedules(due_at, 2).unwrap().is_empty());
        assert!(
            store
                .finish_schedule(&job.id, due_at + 7, "scheduled result")
                .unwrap()
        );
        let finished = store.list_schedules("alice").unwrap().remove(0);
        assert!(!finished.running);
        assert!(finished.enabled);
        assert_eq!(finished.last_result, "scheduled result");
        assert_eq!(finished.next_run_at, due_at + 7 + 300);
        assert!(!store.delete_schedule("bob", &job.id).unwrap());
        assert!(store.delete_schedule("alice", &job.id).unwrap());
        assert!(store.list_schedules("alice").unwrap().is_empty());
    }
    #[test]
    fn wakeup_replaces_only_the_session_pending_wakeup() {
        let temp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&temp.path().join("wakeups.sqlite")).unwrap();
        let when = now() + 3600;
        let a = store
            .create_schedule(
                "alice", "Wake A", "prompt A", "wakeup", when, 0, "", "", "", "", "",
            )
            .unwrap();
        let b = store
            .create_schedule(
                "alice",
                "Wake B",
                "prompt B",
                "wakeup",
                when + 10,
                0,
                "",
                "",
                "",
                "",
                "",
            )
            .unwrap();
        let other = store
            .create_schedule(
                "bob", "Wake B", "prompt B", "wakeup", when, 0, "", "", "", "", "",
            )
            .unwrap();
        let alice = store.list_schedules("alice").unwrap();
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].id, b.id);
        assert_ne!(a.id, alice[0].id);
        assert_eq!(store.clear_wakeups("alice").unwrap(), 1);
        assert_eq!(store.list_schedules("alice").unwrap().len(), 0);
        assert_eq!(store.list_schedules("bob").unwrap()[0].id, other.id);
    }
}
