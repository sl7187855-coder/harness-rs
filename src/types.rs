use crate::{
    config::Config, cordis_runtime::CordisRuntime, memory::MemoryStore, tools::ToolRuntime,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicUsize},
    time::Duration,
};
use tokio::{
    sync::{Mutex, Semaphore, mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub type EventSender = mpsc::Sender<Value>;

pub struct AppState {
    pub config: Config,
    pub memory: MemoryStore,
    pub tools: ToolRuntime,
    pub cordis: CordisRuntime,
    pub http: reqwest::Client,
    pub tasks_lock: Mutex<()>,
    pub history_lock: Mutex<()>,
    pub todos: Mutex<HashMap<String, Vec<Value>>>,
    pub active_tools: Mutex<HashMap<String, HashSet<String>>>,
    pub plan_mode: Mutex<HashSet<String>>,
    pub runs: Mutex<HashMap<String, RunControl>>,
    pub run_requests: Mutex<HashMap<String, ChatRequest>>,
    pub agents: Mutex<HashMap<String, AgentHandle>>,
    pub agent_slots: std::sync::Arc<Semaphore>,
    pub pending: Mutex<HashMap<String, PendingInteraction>>,
    pub provider_profiles: Mutex<HashMap<String, ProviderProfile>>,
    pub provider_limits: Mutex<HashMap<String, Arc<Semaphore>>>,
    pub model_inflight: Arc<AtomicUsize>,
    pub model_usage: Mutex<ModelUsageTotals>,
    pub memory_degraded: Mutex<Option<String>>,
    pub started: Instant,
}
impl AppState {
    pub fn new(
        config: Config,
        memory: MemoryStore,
        tools: ToolRuntime,
        cordis: CordisRuntime,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(2)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let agent_parallelism = config.max_parallel_tools.clamp(1, 4);
        Self {
            config,
            memory,
            tools,
            cordis,
            http,
            tasks_lock: Mutex::new(()),
            history_lock: Mutex::new(()),
            todos: Mutex::new(HashMap::new()),
            active_tools: Mutex::new(HashMap::new()),
            plan_mode: Mutex::new(HashSet::new()),
            runs: Mutex::new(HashMap::new()),
            run_requests: Mutex::new(HashMap::new()),
            agents: Mutex::new(HashMap::new()),
            agent_slots: Arc::new(Semaphore::new(agent_parallelism)),
            pending: Mutex::new(HashMap::new()),
            provider_profiles: Mutex::new(HashMap::new()),
            provider_limits: Mutex::new(HashMap::new()),
            model_inflight: Arc::new(AtomicUsize::new(0)),
            model_usage: Mutex::new(ModelUsageTotals::default()),
            memory_degraded: Mutex::new(None),
            started: Instant::now(),
        }
    }
    pub async fn active_names(&self, session: &str) -> Vec<String> {
        let mut map = self.active_tools.lock().await;
        let entry = map.entry(session.to_owned()).or_insert_with(|| {
            crate::tool_catalog::ready_names()
                .into_iter()
                .map(str::to_owned)
                .collect()
        });
        entry.iter().cloned().collect()
    }
}
pub struct RunControl {
    pub cancel: CancellationToken,
    pub started: Instant,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub parent_session: String,
    pub parent_run_id: String,
    pub session_id: String,
    pub run_id: String,
    pub description: String,
    pub status: String,
    pub allowed_tools: Vec<String>,
    pub result: String,
    pub error: String,
    pub messages: Vec<String>,
    pub started_at: i64,
    pub finished_at: i64,
}
pub struct AgentHandle {
    pub record: AgentRecord,
    pub cancel: CancellationToken,
}
pub struct PendingInteraction {
    pub run_id: String,
    pub session_id: String,
    pub kind: String,
    pub request: Value,
    pub sender: oneshot::Sender<Value>,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct ModelUsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub estimated_cost_usd: f64,
    pub priced_requests: u64,
    pub unpriced_requests: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ChatRequest {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub history_id: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_id: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub global_instructions: String,
    #[serde(default)]
    pub history: Vec<Value>,
    #[serde(default)]
    pub attachments: Vec<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl ChatRequest {
    pub fn session(&self, run_id: &str) -> String {
        let id = if !self.history_id.trim().is_empty() {
            &self.history_id
        } else if !self.session_id.trim().is_empty() {
            &self.session_id
        } else {
            run_id
        };
        id.chars().take(128).collect()
    }
}
pub fn trim_json_output(value: &Value, cap: usize) -> Value {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    if serialized.len() <= cap {
        return value.clone();
    }
    serde_json::json!({"truncated":true,"original_bytes":serialized.len(),"preview":serialized.chars().take(cap).collect::<String>()})
}
