use crate::{
    agent, tool_catalog,
    types::{AppState, ChatRequest, ProviderProfile, RunControl},
};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{
    convert::Infallible,
    fs,
    path::Path as FsPath,
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

include!("server_extra.rs");

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(hud))
        .route("/api/health", get(health))
        .route("/api/harness/health", get(health))
        .route("/api/harness/tools", get(tools))
        .route("/api/harness/metrics", get(metrics))
        .route("/api/harness/context", get(context_metrics))
        .route("/api/dsh-chat", post(dsh_chat))
        .route("/api/dsh-run", get(dsh_run).post(dsh_chat))
        .route("/api/dsh-stop", post(dsh_stop))
        .route("/api/dsh-respond", post(dsh_respond))
        .route("/api/dsh-pending", get(dsh_pending))
        .route("/api/dsh-status", get(dsh_status))
        .route("/api/settings", get(get_settings).post(save_settings))
        .route("/api/memory", get(memory_api).post(memory_save))
        .route("/api/memory/{id}", delete(memory_delete))
        .route("/api/history", get(history_api).post(history_upsert))
        .route("/api/history-delete", post(history_delete))
        .route("/api/history-clear", post(history_clear))
        .route("/api/dsh-warm", post(dsh_warm))
        .route("/api/warm-status", get(warm_status))
        .route("/api/harness-effort", get(get_effort).post(set_effort))
        .route("/api/local-models", get(local_models))
        .route("/api/discover-key", post(discover_key))
        .route("/api/chat-mesh", get(chat_mesh))
        .route(
            "/api/notification-capabilities",
            get(notification_capabilities),
        )
        .route("/api/system-storage", get(system_storage))
        .route("/api/uploads", get(list_uploads))
        .route("/api/download", get(download_file))
        .route("/api/upload", post(upload_file))
        .route("/api/upload-delete", post(upload_delete))
        .route("/api/workspace/photos", get(workspace_photos))
        .route("/api/plugin-install", post(plugin_install))
        .route("/api/learning-candidate", post(learning_candidate))
        .route("/api/learning-reset", post(learning_reset))
        .route("/api/browser/status", get(browser_status))
        .route("/api/browser/navigate", post(browser_navigate))
        .route("/api/browser/input", post(browser_input))
        .route("/api/browser/frame", get(browser_frame))
        .route("/frame", get(browser_frame))
        .route("/api/mobile/apk", get(mobile_apk))
        .route("/api/mobile/frame", get(mobile_frame))
        .route("/api/research-image", get(research_image))
        .route("/api/research-images", get(research_images))
        .route("/api/research-media", get(research_media))
        .route(
            "/api/voice/config",
            get(voice_config).post(voice_config_post),
        )
        .route("/api/voice/voices", get(voice_voices))
        .route("/api/voice/speak", post(voice_speak))
        .route("/voice", get(voice_root).post(voice_root_post))
        .route("/voice/{*path}", get(voice_proxy).post(voice_proxy_post))
        .route("/api/workspace/terminal/raw/", get(terminal_raw_root))
        .route("/api/workspace/terminal/raw/{*path}", get(terminal_raw))
        .route("/api/mobile/devices", get(mobile_devices))
        .route("/api/mobile/command", post(mobile_command))
        .route("/api/mobile/pairing", post(mobile_pairing))
        .route("/api/providers", get(providers))
        .route("/api/models", get(models))
        .route("/api/models-batch", post(models_batch))
        .route("/api/apis", get(list_apis).post(save_api))
        .route("/api/apis/{id}", delete(delete_api))
        .route("/api/harness-plugins", get(plugins))
        .route("/api/harness-skills", get(skills))
        .route("/api/learnings", get(learnings))
        .route("/api/learning-candidates", get(learning_candidates))
        .route("/api/events", get(events))
        .route("/api/workspace/list", get(workspace_list))
        .route("/api/workspace/projects", get(workspace_projects))
        .route("/api/workspace/terminal/tree", get(workspace_tree))
        .route("/api/workspace/terminal/file", get(workspace_file))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(40 * 1024 * 1024))
        .with_state(state)
}

pub fn bridge_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(bridge_health))
        .route("/api/health", get(bridge_health))
        .route("/api/dsh-chat", post(dsh_chat))
        .route("/api/dsh-run", get(dsh_run).post(dsh_chat))
        .route("/api/dsh-stop", post(dsh_stop))
        .route("/api/dsh-respond", post(dsh_respond))
        .route("/api/dsh-pending", get(dsh_pending))
        .route("/api/dsh-status", get(dsh_status))
        .route("/api/harness/tools", get(tools))
        .route("/api/harness/metrics", get(metrics))
        .route("/api/harness/context", get(context_metrics))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(40 * 1024 * 1024))
        .with_state(state)
}

async fn hud(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let accepts_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("text/html");
    match tokio::fs::read(&state.config.hud_file).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| {
                Html("<h1>HARNESS-RS: falha ao enviar HUD</h1>".to_owned()).into_response()
            }),
        Err(e) => {
            let body = if accepts_html {
                format!(
                    "<!doctype html><html lang='pt-BR'><meta charset='utf-8'><title>HARNESS-RS</title><body style='font-family:sans-serif;background:#080808;color:#ddd;padding:3rem'><h1>HARNESS-RS iniciou</h1><p>Não foi possível ler o HUD configurado.</p><pre>{}</pre><p>Configure HARNESS_HUD_FILE para apontar ao thomas-hud.html já existente.</p><p><a href='/api/health'>API health</a></p></body></html>",
                    html_escape(&e.to_string())
                )
            } else {
                "HUD unavailable".into()
            };
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(body))
                .unwrap_or_else(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}

async fn dsh_chat(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<ChatRequest>,
) -> Response {
    let run_id = if request.run_id.trim().is_empty() {
        format!("run-{}", Uuid::new_v4())
    } else {
        request.run_id.trim().to_owned()
    };
    let session_id = request.session(&run_id);
    request.run_id = run_id.clone();
    if is_rewind_command(&request.message) {
        return rewind_command_response(
            state,
            run_id,
            session_id,
            request.message.trim().to_owned(),
        )
        .await;
    }
    let cancel = CancellationToken::new();
    {
        // Check and register atomically. The same map lock is held for a
        // complete /rewind operation, preventing a model run from slipping
        // between the rewind busy check and its own registration.
        let mut runs = state.runs.lock().await;
        if runs.contains_key(&run_id) {
            return (
                StatusCode::CONFLICT,
                Json(json!({"ok":false,"error":"run_id_already_active","run_id":run_id})),
            )
                .into_response();
        }
        runs.insert(
            run_id.clone(),
            RunControl {
                cancel: cancel.clone(),
                started: tokio::time::Instant::now(),
            },
        );
    }
    let (tx, mut rx) = mpsc::channel::<Value>(128);
    let state2 = state.clone();
    let run2 = run_id.clone();
    let session2 = session_id.clone();
    let cancel_guard = cancel.clone();
    tokio::spawn(async move {
        agent::run_turn(state2, request, run2, session2, tx, cancel).await;
    });
    let stream = async_stream::stream! {
        let _guard=CancelOnDrop(cancel_guard);
        while let Some(value)=rx.recv().await {
            yield Ok::<Event,Infallible>(Event::default().data(value.to_string()));
        }
    };
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(8))
                .text("heartbeat"),
        )
        .into_response()
}

fn is_rewind_command(message: &str) -> bool {
    let trimmed = message.trim();
    trimmed == "/rewind" || trimmed.starts_with("/rewind ") || trimmed.starts_with("/rewind\t")
}

async fn rewind_command_response(
    state: Arc<AppState>,
    run_id: String,
    session_id: String,
    command: String,
) -> Response {
    let audit_started = state
        .tools
        .audit_event(&session_id, &run_id, "/rewind", "start", "started", None)
        .is_ok();
    let active_runs = state.runs.lock().await;
    let mut result = if !audit_started {
        Err((
            "AUDIT_UNAVAILABLE",
            "a trilha de auditoria está indisponível; o rewind não foi executado".to_owned(),
        ))
    } else if !active_runs.is_empty() {
        Err((
            "REWIND_BUSY",
            "Existe uma execução ativa; aguarde ela terminar antes de restaurar arquivos."
                .to_owned(),
        ))
    } else {
        state
            .tools
            .rewind_command(&session_id, &command)
            .map_err(|error| (error.code, error.message))
    };
    if audit_started {
        let (outcome, error_code) = match &result {
            Ok(_) => ("completed", None),
            Err((code, _)) => ("failed", Some(*code)),
        };
        if state
            .tools
            .audit_event(
                &session_id,
                &run_id,
                "/rewind",
                "finish",
                outcome,
                error_code,
            )
            .is_err()
        {
            result = Err((
                "AUDIT_COMMIT_FAILED",
                "o rewind pode ter sido executado, mas não foi possível registrar o resultado; verifique o arquivo antes de repetir".to_owned(),
            ));
        }
    }
    drop(active_runs);
    let events = match result {
        Ok(message) => vec![
            json!({"type":"hud_boot","run_id":run_id,"sessionId":session_id,"harness":"HARNESS-RS","version":"0.1.0"}),
            json!({"type":"run_registered","run_id":run_id,"sessionId":session_id}),
            json!({"type":"status","phase":"rewind_command","label":"Comando local /rewind"}),
            json!({"type":"text_delta","text":message,"run_id":run_id}),
            json!({"type":"turn_end","reason":{"kind":"completed","message":"rewind_command"}}),
        ],
        Err((code, message)) => vec![
            json!({"type":"hud_boot","run_id":run_id,"sessionId":session_id,"harness":"HARNESS-RS","version":"0.1.0"}),
            json!({"type":"run_registered","run_id":run_id,"sessionId":session_id}),
            json!({"type":"text_delta","text":format!("Não executei o /rewind ({code}): {message}"),"run_id":run_id}),
            json!({"type":"turn_end","reason":{"kind":"completed","message":"rewind_command_rejected"}}),
        ],
    };
    let stream = async_stream::stream! {
        for value in events {
            yield Ok::<Event, Infallible>(Event::default().data(value.to_string()));
        }
    };
    Sse::new(stream).into_response()
}

struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(serde::Deserialize)]
struct StopRequest {
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    session_id: String,
}
async fn dsh_stop(
    State(state): State<Arc<AppState>>,
    Json(request): Json<StopRequest>,
) -> Response {
    let mut cancelled = false;
    {
        let runs = state.runs.lock().await;
        if !request.run_id.is_empty() {
            if let Some(run) = runs.get(&request.run_id) {
                run.cancel.cancel();
                cancelled = true;
            }
        } else {
            for (id, run) in runs.iter() {
                if id == &request.session_id {
                    run.cancel.cancel();
                    cancelled = true;
                }
            }
        }
    }
    (
        StatusCode::OK,
        Json(json!({"ok":true,"cancelled":cancelled,"run_id":request.run_id})),
    )
        .into_response()
}

async fn dsh_respond(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let id = body
        .get("id")
        .or_else(|| body.get("eventId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"id is required"})),
        )
            .into_response();
    }
    let outcome = if let Some(raw) = body.get("outcome") {
        if let Some(s) = raw.as_str() {
            json!({"outcome":s})
        } else {
            raw.clone()
        }
    } else if let Some(answers) = body.get("answers") {
        json!({"answers":answers})
    } else if let Some(approved) = body.get("approved").and_then(Value::as_bool) {
        json!({"approved":approved,"outcome":if approved{"allowed-once"}else{"rejected"}})
    } else {
        json!({})
    };
    let pending = state.pending.lock().await.remove(&id);
    match pending {
        Some(p) => match p.sender.send(outcome) {
            Ok(()) => (StatusCode::OK, Json(json!({"ok":true,"id":id}))).into_response(),
            Err(_) => (
                StatusCode::CONFLICT,
                Json(json!({"ok":false,"error":"interaction closed","run_id":p.run_id})),
            )
                .into_response(),
        },
        None => (
            StatusCode::CONFLICT,
            Json(json!({"ok":false,"error":"no pending interaction","id":id})),
        )
            .into_response(),
    }
}

async fn dsh_pending(State(state): State<Arc<AppState>>) -> Response {
    let pending = state.pending.lock().await;
    let requests=pending.iter().map(|(id,p)|{
        let event=if p.kind=="approval"{"approval/request"}else{"user-questions/request"};
        json!({"id":id,"eventId":id,"run_id":p.run_id,"runId":p.run_id,"sessionId":p.session_id,"event":event,"request":p.request})
    }).collect::<Vec<_>>();
    Json(json!({"ok":true,"requests":requests,"pending":requests,"count":requests.len()}))
        .into_response()
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    let tool_count = tool_catalog::all().len();
    let ready = tool_catalog::all().iter().filter(|t| t.ready()).count();
    let mut memory = state
        .memory
        .counts()
        .unwrap_or_else(|e| json!({"error":e.to_string()}));
    let memory_error = state.memory_degraded.lock().await.clone();
    if let Some(obj) = memory.as_object_mut() {
        let failed = memory_error.is_some() || obj.contains_key("error");
        obj.insert("degraded".into(), json!(failed));
        if let Some(reason) = memory_error {
            obj.insert(
                "degraded_reason".into(),
                json!(reason.chars().take(300).collect::<String>()),
            );
        }
    }
    let rss = rss_bytes();
    let cgroup = cgroup_memory_bytes();
    let audit = state.tools.audit_status();
    let audit_ok = audit.healthy;
    let max = 1536u64 * 1024 * 1024;
    Json(json!({"ok":audit_ok,"service":"harness-rs","version":env!("CARGO_PKG_VERSION"),"build":if cfg!(debug_assertions){"debug"}else{"release"},"runtime":"Rust + cordis-rs","tool_count":tool_count,"tool_ready":ready,"tool_deferred":tool_count-ready,"memory":memory,"audit":audit,"ram":{"process_rss_bytes":rss,"cgroup_current_bytes":cgroup,"budget_bytes":max,"within_budget":cgroup.map(|v|v<=max).unwrap_or_else(||rss.is_some_and(|v|v<=max))},"uptime_seconds":state.started.elapsed().as_secs(),"workspace":state.config.workspace,"bridge":"rust-hud-compat","old_dsh_disabled":true})).into_response()
}
async fn bridge_health(State(state): State<Arc<AppState>>) -> Response {
    Json(json!({"ok":true,"service":"harness-rs-bridge","target":"harness-rs-core","tool_count":tool_catalog::all().len(),"uptime_seconds":state.started.elapsed().as_secs()})).into_response()
}
async fn tools() -> Response {
    let all = tool_catalog::all();
    Json(json!({"ok":true,"count":all.len(),"tools":all})).into_response()
}
async fn metrics(State(state): State<Arc<AppState>>) -> Response {
    let rss = rss_bytes();
    let cg = cgroup_memory_bytes();
    let counts = state.memory.counts().unwrap_or(Value::Null);
    let usage = *state.model_usage.lock().await;
    Json(json!({"ok":true,"process_rss_bytes":rss,"cgroup_current_bytes":cg,"budget_bytes":1536u64*1024*1024,"uptime_seconds":state.started.elapsed().as_secs(),"storage":counts,"active_runs":state.runs.lock().await.len(),"pending_interactions":state.pending.lock().await.len(),"tool_catalog_count":tool_catalog::all().len(),"model_inflight":state.model_inflight.load(Ordering::Relaxed),"model_usage":usage})).into_response()
}
async fn get_settings(State(state): State<Arc<AppState>>) -> Response {
    let path = state.config.data_dir.join("settings.json");
    let mut value = tokio::fs::read_to_string(path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or(json!({}));
    let obj = value.as_object_mut().unwrap();
    obj.entry("event_scheduler_enabled").or_insert(json!(true));
    obj.entry("memory_enabled").or_insert(json!(true));
    obj.entry("show_tool_trace").or_insert(json!(true));
    obj.entry("compact").or_insert(json!(true));
    obj.insert(
        "execution_mode".into(),
        json!(crate::runtime_settings::execution_mode(
            &state.config.data_dir
        )),
    );
    obj.entry("api_base_url")
        .or_insert(json!(state.config.api_base_url));
    obj.entry("model").or_insert(json!(state.config.model));
    obj.entry("theme").or_insert(json!("dark"));
    obj.insert("ok".into(), json!(true));
    Json(value).into_response()
}

async fn save_settings(State(state): State<Arc<AppState>>, Json(value): Json<Value>) -> Response {
    if !value.is_object() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"settings must be an object"})),
        )
            .into_response();
    }
    if let Some(mode) = value.get("execution_mode") {
        if !mode
            .as_str()
            .is_some_and(crate::runtime_settings::validate_execution_mode)
        {
            return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"execution_mode must be one of plan, ask, auto-edit, auto"}))).into_response();
        }
    }
    let mut clean = value.clone();
    if let Some(map) = clean.as_object_mut() {
        map.remove("ok");
    }
    let path = state.config.data_dir.join("settings.json");
    match atomic_json_write(&path, &clean).await {
        Ok(()) => {
            let mut out = clean;
            out.as_object_mut()
                .unwrap()
                .insert("ok".into(), json!(true));
            Json(out).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}

async fn providers() -> Response {
    Json(json!({"ok":true,"providers":[
        {"id":"deepseek","name":"DeepSeek","base_url":"https://api.deepseek.com/v1","api_required":true,"format":"openai-compatible"},
        {"id":"openai","name":"OpenAI","base_url":"https://api.openai.com/v1","api_required":true,"format":"openai-compatible"},
        {"id":"openrouter","name":"OpenRouter","base_url":"https://openrouter.ai/api/v1","api_required":true,"format":"openai-compatible"},
        {"id":"gemini","name":"Google Gemini","base_url":"https://generativelanguage.googleapis.com/v1beta/openai","api_required":true,"format":"openai-compatible"},
        {"id":"anthropic","name":"Anthropic","base_url":"https://api.anthropic.com/v1","api_required":true,"format":"openai-compatible"},
        {"id":"custom","name":"OpenAI-compatible","base_url":"","api_required":true,"format":"openai-compatible"}
    ]})).into_response()
}

#[derive(serde::Deserialize, Default)]
struct ModelQuery {
    #[serde(default)]
    provider: String,
    #[serde(default)]
    api_id: String,
}
fn fallback_models(provider: &str) -> Vec<String> {
    match provider.to_ascii_lowercase().as_str() {
        "openai" => vec![
            "gpt-4.1".into(),
            "gpt-4.1-mini".into(),
            "gpt-4o-mini".into(),
        ],
        "openrouter" => vec![
            "deepseek/deepseek-chat".into(),
            "openai/gpt-4.1-mini".into(),
        ],
        "gemini" | "google" => vec!["gemini-2.5-flash".into(), "gemini-2.5-pro".into()],
        "anthropic" => vec!["claude-sonnet-4".into(), "claude-haiku-4-5".into()],
        _ => vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
    }
}
async fn discover_models_for_profile(
    state: &Arc<AppState>,
    profile: &ProviderProfile,
) -> Vec<String> {
    if profile.api_key.trim().is_empty() {
        return fallback_models(&profile.provider);
    }
    let base = profile
        .base_url
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions");
    if !base.starts_with("https://")
        && !base.starts_with("http://127.0.0.1")
        && !base.starts_with("http://localhost")
    {
        return Vec::new();
    }
    let url = format!("{base}/models");
    let result = tokio::time::timeout(
        Duration::from_secs(6),
        state.http.get(url).bearer_auth(&profile.api_key).send(),
    )
    .await;
    if let Ok(Ok(response)) = result {
        if response.status().is_success() {
            if let Ok(body) = response.json::<Value>().await {
                if let Some(data) = body.get("data").and_then(Value::as_array) {
                    let mut ids = data
                        .iter()
                        .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_owned))
                        .collect::<Vec<_>>();
                    ids.sort();
                    ids.dedup();
                    ids.truncate(200);
                    if !ids.is_empty() {
                        return ids;
                    }
                }
            }
        }
    }
    fallback_models(&profile.provider)
}
async fn models(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<ModelQuery>,
) -> Response {
    let profiles = state.provider_profiles.lock().await;
    let profile = if !q.api_id.is_empty() {
        profiles.get(&q.api_id).cloned()
    } else {
        profiles
            .values()
            .find(|p| p.provider == q.provider)
            .cloned()
    };
    drop(profiles);
    let provider = profile
        .as_ref()
        .map(|p| p.provider.clone())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| {
            if q.provider.is_empty() {
                "deepseek".into()
            } else {
                q.provider.clone()
            }
        });
    let ids = if let Some(p) = profile.as_ref() {
        discover_models_for_profile(&state, p).await
    } else {
        fallback_models(&provider)
    };
    let models = ids
        .into_iter()
        .map(|id| json!({"id":id,"model":id,"name":id}))
        .collect::<Vec<_>>();
    Json(json!({"ok":true,"provider":provider,"models":models,"default_model":state.config.model}))
        .into_response()
}

async fn models_batch(State(state): State<Arc<AppState>>, Json(_body): Json<Value>) -> Response {
    let profiles = state
        .provider_profiles
        .lock()
        .await
        .values()
        .cloned()
        .take(20)
        .collect::<Vec<_>>();
    let jobs = profiles
        .iter()
        .map(|p| discover_models_for_profile(&state, p));
    let resolved = futures_util::future::join_all(jobs).await;
    let mut models = Vec::new();
    for (profile, ids) in profiles.iter().zip(resolved) {
        for id in ids {
            models.push(json!({"id":id,"model":id,"name":id,"provider":profile.provider,"provider_id":profile.provider,"provider_name":profile.name,"api_id":profile.id}));
        }
    }
    Json(json!({"ok":true,"models":models,"count":models.len()})).into_response()
}

async fn list_apis(State(state): State<Arc<AppState>>) -> Response {
    let active = tokio::fs::read_to_string(state.config.data_dir.join("settings.json"))
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.get("thomasApiId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let profiles=state.provider_profiles.lock().await.values().map(|p|{
        let masked=if p.api_key.len()>4{format!("••••{}",p.api_key.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>())}else if p.api_key.is_empty(){"not configured".into()}else{"••••".into()};
        json!({"id":p.id,"provider":p.provider,"name":p.name,"label":p.name,"base_url":p.base_url,"model":p.model,"masked":masked,"has_api_key":!p.api_key.is_empty(),"active":p.id==active})
    }).collect::<Vec<_>>();
    Json(json!({"ok":true,"apis":profiles,"count":profiles.len()})).into_response()
}

async fn save_api(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let action = body.get("action").and_then(Value::as_str).unwrap_or("");
    let id = body
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if action == "use" {
        let profiles = state.provider_profiles.lock().await;
        let Some(profile) = profiles.get(&id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"ok":false,"error":"API not found"})),
            )
                .into_response();
        };
        let provider = profile.provider.clone();
        drop(profiles);
        let path = state.config.data_dir.join("settings.json");
        let mut settings = tokio::fs::read_to_string(&path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        settings["thomasApiId"] = json!(id);
        settings["thomasProvider"] = json!(provider);
        return match atomic_json_write(&path, &settings).await {
            Ok(()) => Json(json!({"ok":true,"id":id,"active":true})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok":false,"error":e})),
            )
                .into_response(),
        };
    }
    if action == "delete" {
        return match remove_api_profile(&state, &id).await {
            Ok(removed) => Json(json!({"ok":true,"removed":removed,"id":id})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok":false,"error":e})),
            )
                .into_response(),
        };
    }

    let provider = body
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let api_key = body
        .get("api_key")
        .or_else(|| body.get("apiKey"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let label = body
        .get("label")
        .or_else(|| body.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if provider.is_empty() || api_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"provider and api_key are required"})),
        )
            .into_response();
    }
    if provider.len() > 80 || api_key.len() > 4096 || label.len() > 120 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"provider, key or label exceeds a safe limit"})),
        )
            .into_response();
    }
    let base_url = body
        .get("base_url")
        .or_else(|| body.get("baseURL"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let base_url = if base_url.is_empty() {
        provider_base_url(&provider).to_owned()
    } else {
        base_url
    };
    if !base_url.starts_with("https://")
        && !base_url.starts_with("http://127.0.0.1")
        && !base_url.starts_with("http://localhost")
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"provider base_url must use HTTPS except on loopback"})),
        )
            .into_response();
    }
    let existing_id = {
        let profiles = state.provider_profiles.lock().await;
        profiles
            .values()
            .find(|p| p.provider == provider && p.api_key == api_key)
            .map(|p| p.id.clone())
    };
    let was_existing = existing_id.is_some();
    let api_id = existing_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    let profile = ProviderProfile {
        id: api_id.clone(),
        provider: provider.clone(),
        name: if label.is_empty() {
            provider_label(&provider).to_owned()
        } else {
            label
        },
        base_url,
        model: body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        api_key,
    };
    let file = state.config.data_dir.join("providers.json");
    let mut profiles = state.provider_profiles.lock().await;
    profiles.insert(api_id.clone(), profile.clone());
    let serialized =
        serde_json::to_value(profiles.values().cloned().collect::<Vec<_>>()).unwrap_or(json!([]));
    drop(profiles);
    match atomic_json_write(&file, &serialized).await {
        Ok(()) => {
            let _ = tokio::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).await;
            let _ = save_active_api(&state, &api_id, &provider).await;
            let masked = profile
                .api_key
                .chars()
                .rev()
                .take(4)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            Json(json!({"ok":true,"api":{"id":api_id,"provider":provider,"name":profile.name,"label":profile.name,"masked":format!("••••{masked}"),"base_url":profile.base_url,"existing":was_existing},"has_api_key":true})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
async fn save_active_api(state: &Arc<AppState>, id: &str, provider: &str) -> Result<(), String> {
    let path = state.config.data_dir.join("settings.json");
    let mut settings = tokio::fs::read_to_string(&path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or(json!({}));
    settings["thomasApiId"] = json!(id);
    settings["thomasProvider"] = json!(provider);
    atomic_json_write(&path, &settings).await
}
async fn remove_api_profile(state: &Arc<AppState>, id: &str) -> Result<bool, String> {
    let mut profiles = state.provider_profiles.lock().await;
    let removed = profiles.remove(id).is_some();
    let serialized =
        serde_json::to_value(profiles.values().cloned().collect::<Vec<_>>()).unwrap_or(json!([]));
    drop(profiles);
    let file = state.config.data_dir.join("providers.json");
    atomic_json_write(&file, &serialized).await?;
    if let Ok(text) = tokio::fs::read_to_string(state.config.data_dir.join("settings.json")).await {
        if let Ok(mut settings) = serde_json::from_str::<Value>(&text) {
            if settings.get("thomasApiId").and_then(Value::as_str) == Some(id) {
                settings["thomasApiId"] = json!("");
                let _ = atomic_json_write(&state.config.data_dir.join("settings.json"), &settings)
                    .await;
            }
        }
    }
    Ok(removed)
}
fn provider_base_url(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com/v1",
        "openai" => "https://api.openai.com/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "gemini" | "google" => "https://generativelanguage.googleapis.com/v1beta/openai",
        "anthropic" => "https://api.anthropic.com/v1",
        _ => "https://api.deepseek.com/v1",
    }
}
fn provider_label(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "DeepSeek",
        "openai" => "OpenAI",
        "openrouter" => "OpenRouter",
        "gemini" | "google" => "Google Gemini",
        "anthropic" => "Anthropic",
        _ => "Custom provider",
    }
}

async fn delete_api(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match remove_api_profile(&state, &id).await {
        Ok(removed) => (
            StatusCode::OK,
            Json(json!({"ok":true,"removed":removed,"id":id})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}

pub async fn load_provider_profiles(state: &Arc<AppState>) {
    let path = state.config.data_dir.join("providers.json");
    if let Ok(text) = tokio::fs::read_to_string(path).await {
        if let Ok(items) = serde_json::from_str::<Vec<ProviderProfile>>(&text) {
            let mut profiles = state.provider_profiles.lock().await;
            for mut p in items.into_iter().take(50) {
                if p.provider.is_empty() {
                    p.provider = infer_provider(&p.base_url);
                }
                if p.name.is_empty() {
                    p.name = provider_label(&p.provider).into();
                }
                if p.base_url.is_empty() {
                    p.base_url = provider_base_url(&p.provider).into();
                }
                profiles.insert(p.id.clone(), p);
            }
        }
    }
}
fn infer_provider(base_url: &str) -> String {
    if base_url.contains("openrouter.ai") {
        "openrouter".into()
    } else if base_url.contains("openai.com") {
        "openai".into()
    } else if base_url.contains("deepseek.com") {
        "deepseek".into()
    } else if base_url.contains("generativelanguage.googleapis.com") {
        "gemini".into()
    } else if base_url.contains("anthropic.com") {
        "anthropic".into()
    } else {
        "custom".into()
    }
}

async fn plugins(State(state): State<Arc<AppState>>) -> Response {
    let manifests = discover_plugin_manifests(&state.config.plugins_dir);
    let enabled = tokio::fs::read_to_string(state.config.data_dir.join("plugins-enabled.json"))
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default();
    let plugins = manifests
        .into_iter()
        .map(|mut p| {
            let is_enabled = enabled
                .iter()
                .any(|id| p.get("id").and_then(Value::as_str) == Some(id.as_str()));
            let kind = p
                .get("type")
                .or_else(|| p.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            p["enabled"] = json!(is_enabled);
            p["runtime_state"] = json!(if !is_enabled {
                "disabled"
            } else if kind == "context" || kind == "declarative-context" {
                "active_context"
            } else {
                "enabled_manifest_only"
            });
            p["capabilities"] = if kind == "context" || kind == "declarative-context" {
                json!(["system_prompt", "restricted_ready_tools"])
            } else {
                json!([])
            };
            p
        })
        .collect::<Vec<_>>();
    Json(json!({"ok":true,"plugins":plugins,"count":plugins.len(),"runtime":"cordis-rs","execution":"Declarative context/tool plugins are supported; arbitrary executable plugins and native MCP adapters remain disabled."})).into_response()
}
fn discover_plugin_manifests(root: &FsPath) -> Vec<Value> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return result;
    };
    for entry in entries.flatten().take(100) {
        let folder = entry.path();
        if !folder.is_dir() {
            continue;
        }
        for candidate in [folder.join("plugin.json"), folder.join("manifest.json")] {
            if !candidate.is_file() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&candidate) else {
                continue;
            };
            let Ok(mut m) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            if !m.is_object() {
                continue;
            }
            let id = m
                .get("id")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
            if id.len() > 96 || id.contains('/') || id.contains('\\') {
                continue;
            }
            let name = m
                .get("name")
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .unwrap_or(&id)
                .to_owned();
            m["id"] = json!(id);
            m["name"] = json!(name);
            m["manifest_path"] = json!(candidate.to_string_lossy().to_string());
            m["manifest_valid"] = json!(true);
            m["runtime_state"] = json!("manifest_only");
            m["enabled"] = json!(false);
            if !m.get("version").is_some_and(Value::is_string) {
                m["version"] = json!("0.0.0");
            }
            if !m.get("type").is_some_and(Value::is_string) {
                m["type"] = json!("metadata");
            }
            result.push(m);
            break;
        }
    }
    result.sort_by(|a, b| {
        a.get("id")
            .and_then(Value::as_str)
            .cmp(&b.get("id").and_then(Value::as_str))
    });
    result
}
async fn skills(State(state): State<Arc<AppState>>) -> Response {
    let mut results = Vec::new();
    for base in [
        state.config.workspace.join(".claude/skills"),
        state.config.workspace.join(".agents/skills"),
        state.config.workspace.join(".harness/skills"),
        state.config.skills_dir.clone(),
    ] {
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten().take(200) {
                if entry.path().is_dir() && entry.path().join("SKILL.md").exists() {
                    results.push(json!({"name":entry.file_name().to_string_lossy(),"path":entry.path().to_string_lossy()}));
                }
            }
        }
    }
    Json(json!({"ok":true,"skills":results,"count":results.len()})).into_response()
}
async fn learnings(State(state): State<Arc<AppState>>) -> Response {
    let items=state.memory.list_memories(200).unwrap_or_default().into_iter().map(|m|{
        let kind=m.get("kind").and_then(Value::as_str).unwrap_or("fact");
        json!({"id":m["id"],"kind":kind,"title":m["title"],"content":m["text"],"status":"active","confidence":m["confidence"],"updated_at":m["updated_at"],"trigger":m["scope"]})
    }).collect::<Vec<_>>();
    let candidates =
        tokio::fs::read_to_string(state.config.data_dir.join("learning-candidates.json"))
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
            .unwrap_or_default();
    let counts = state.memory.counts().unwrap_or(Value::Null);
    Json(json!({"ok":true,"counts":{"traces":0,"episodes":0,"memories":items.len(),"candidates":candidates.len()},"learning":{"eligible_episodes":0,"reviewed":0},"candidates":candidates,"items":items,"recent_experiences":[],"reviewed_experiences":[],"memory":counts,"status":"sqlite_fts5"})).into_response()
}

async fn learning_candidates(State(state): State<Arc<AppState>>) -> Response {
    let candidates =
        tokio::fs::read_to_string(state.config.data_dir.join("learning-candidates.json"))
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
            .unwrap_or_default();
    Json(json!({"ok":true,"candidates":candidates,"count":candidates.len()})).into_response()
}

async fn events() -> Response {
    Json(json!({"ok":true,"events":[],"count":0,"reason":"scheduler will be enabled in a later increment"})).into_response()
}

async fn workspace_list(State(state): State<Arc<AppState>>) -> Response {
    let dir = state.config.workspace.clone();
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten().take(300) {
            let path = entry.path();
            items.push(json!({"name":entry.file_name().to_string_lossy(),"path":path.to_string_lossy(),"kind":if path.is_dir(){"directory"}else{"file"}}));
        }
    }
    Json(json!({"ok":true,"path":dir,"items":items,"count":items.len()})).into_response()
}
async fn workspace_projects(State(state): State<Arc<AppState>>) -> Response {
    Json(json!({"ok":true,"projects":[{"id":"default","name":state.config.workspace.file_name().and_then(|x|x.to_str()).unwrap_or("workspace"),"path":state.config.workspace}]})).into_response()
}
async fn workspace_tree(State(state): State<Arc<AppState>>) -> Response {
    let mut items = Vec::new();
    let root = state.config.workspace.clone();
    let mut walker = ignore::WalkBuilder::new(&root);
    walker
        .hidden(false)
        .standard_filters(true)
        .max_depth(Some(4))
        .follow_links(false);
    for entry in walker.build().take(500) {
        if let Ok(entry) = entry {
            let p = entry.path();
            if p == root {
                continue;
            }
            items.push(json!({"name":p.file_name().and_then(|s|s.to_str()).unwrap_or("entry"),"path":p.to_string_lossy(),"kind":if p.is_dir(){"directory"}else{"file"}}));
        }
    }
    Json(json!({"ok":true,"root":root,"items":items,"count":items.len()})).into_response()
}
#[derive(serde::Deserialize)]
struct FileQuery {
    path: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: usize,
}
async fn workspace_file(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<FileQuery>,
) -> Response {
    let raw = std::path::PathBuf::from(&q.path);
    let path = if raw.is_absolute() {
        raw
    } else {
        state.config.workspace.join(raw)
    };
    let canon = match fs::canonicalize(&path) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"ok":false,"error":e.to_string()})),
            )
                .into_response();
        }
    };
    let root = fs::canonicalize(&state.config.workspace)
        .unwrap_or_else(|_| state.config.workspace.clone());
    if !canon.starts_with(root) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok":false,"error":"path outside workspace"})),
        )
            .into_response();
    }
    if !canon.is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"not a file"})),
        )
            .into_response();
    }
    let text = match fs::read_to_string(&canon) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Json(json!({"ok":false,"error":e.to_string()})),
            )
                .into_response();
        }
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = q.offset.max(1);
    let limit = q.limit.clamp(1, 1000);
    let end = (start - 1 + limit).min(lines.len());
    Json(json!({"ok":true,"path":canon,"content":lines.iter().enumerate().skip(start-1).take(limit).map(|(i,l)|format!("{:>5} {}",i+1,l)).collect::<Vec<_>>().join("\n"),"total_lines":lines.len(),"truncated":end<lines.len()})).into_response()
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND,Json(json!({"ok":false,"reason":"not_implemented","message":"route is not implemented in HARNESS-RS yet"}))).into_response()
}
fn rss_bytes() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}
fn cgroup_memory_bytes() -> Option<u64> {
    // Prefer the cgroup that contains this process, not the host/container root.
    // On this host, memory is a v1 controller mounted at /sys/fs/cgroup/memory.
    if let Ok(groups) = fs::read_to_string("/proc/self/cgroup") {
        for line in groups.lines() {
            let Some((_, rest)) = line.split_once(':') else {
                continue;
            };
            let Some((controllers, group)) = rest.split_once(':') else {
                continue;
            };
            let relative = group.trim_start_matches('/');
            if controllers.split(',').any(|c| c == "memory") {
                let dir = if relative.is_empty() {
                    "/sys/fs/cgroup/memory".to_owned()
                } else {
                    format!("/sys/fs/cgroup/memory/{relative}")
                };
                if let Ok(s) = fs::read_to_string(format!("{dir}/memory.usage_in_bytes")) {
                    if let Ok(v) = s.trim().parse::<u64>() {
                        return Some(v);
                    }
                }
            } else if controllers.is_empty() {
                for root in ["/sys/fs/cgroup/unified", "/sys/fs/cgroup"] {
                    let dir = if relative.is_empty() {
                        root.to_owned()
                    } else {
                        format!("{root}/{relative}")
                    };
                    if let Ok(s) = fs::read_to_string(format!("{dir}/memory.current")) {
                        if let Ok(v) = s.trim().parse::<u64>() {
                            return Some(v);
                        }
                    }
                }
            }
        }
    }
    // Compatibility fallbacks for hosts that do not expose process membership.
    for path in [
        "/sys/fs/cgroup/memory.current",
        "/sys/fs/cgroup/memory/memory.usage_in_bytes",
    ] {
        if let Ok(s) = fs::read_to_string(path) {
            if let Ok(v) = s.trim().parse::<u64>() {
                return Some(v);
            }
        }
    }
    None
}
async fn atomic_json_write(path: &FsPath, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    use std::io::Write as _;
    file.write_all(
        serde_json::to_string_pretty(value)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.error.to_string())?;
    Ok(())
}
fn html_escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
#[derive(serde::Deserialize, Default)]
struct RunQuery {
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    after_seq: u64,
}
async fn dsh_run(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<RunQuery>,
) -> Response {
    let runs = state.runs.lock().await;
    let active = runs.contains_key(&q.run_id);
    Json(json!({"ok":true,"run_id":q.run_id,"events":[],"after_seq":q.after_seq,"terminal":!active,"active":active,"session_id":""})).into_response()
}

async fn dsh_warm(Json(body): Json<Value>) -> Response {
    let session = body
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            body.get("history_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| format!("session-{}", Uuid::new_v4()));
    Json(json!({"ok":true,"sessionId":session,"warmed":false,"status":"ready","detail":"HARNESS-RS uses model calls on demand; no fake warm session."})).into_response()
}
async fn warm_status() -> Response {
    Json(json!({"ok":true,"status":"idle","warming":false,"warmed":false,"service":"harness-rs"}))
        .into_response()
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    #[serde(default)]
    id: Option<String>,
}
async fn history_api(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<HistoryQuery>,
) -> Response {
    let _guard = state.history_lock.lock().await;
    let mut items = read_history(&state).await;
    if let Some(id) = q.id.filter(|id| !id.is_empty()) {
        return match items
            .into_iter()
            .find(|v| v.get("id").and_then(Value::as_str) == Some(id.as_str()))
        {
            Some(mut item) => {
                item["ok"] = json!(true);
                Json(item).into_response()
            }
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({"ok":false,"error":"conversation_not_found","id":id})),
            )
                .into_response(),
        };
    }
    items.sort_by(|a, b| {
        b.get("updated_at")
            .and_then(Value::as_i64)
            .cmp(&a.get("updated_at").and_then(Value::as_i64))
    });
    let count = items.len();
    Json(json!({"ok":true,"conversations":items,"count":count})).into_response()
}
async fn history_upsert(
    State(state): State<Arc<AppState>>,
    Json(mut item): Json<Value>,
) -> Response {
    if !item.is_object() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"history entry must be an object"})),
        )
            .into_response();
    }
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    if id.len() > 128 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"conversation id too long"})),
        )
            .into_response();
    }
    let messages = item
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if messages.len() > 500 {
        return (StatusCode::PAYLOAD_TOO_LARGE,Json(json!({"ok":false,"error":"a conversation may contain at most 500 messages per history write"}))).into_response();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    item["id"] = json!(id);
    if !item.get("created_at").is_some_and(Value::is_number) {
        item["created_at"] = json!(now);
    }
    if !item.get("updated_at").is_some_and(Value::is_number) {
        item["updated_at"] = json!(now);
    }
    if item
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        item["title"] = json!("Conversation");
    }
    item["messages"] = json!(messages);
    {
        let _guard = state.history_lock.lock().await;
        let mut items = read_history(&state).await;
        if let Some(existing) = items
            .iter_mut()
            .find(|v| v.get("id").and_then(Value::as_str) == item.get("id").and_then(Value::as_str))
        {
            *existing = item.clone();
        } else {
            items.push(item.clone());
        }
        if items.len() > 200 {
            items.sort_by(|a, b| {
                b.get("updated_at")
                    .and_then(Value::as_i64)
                    .cmp(&a.get("updated_at").and_then(Value::as_i64))
            });
            items.truncate(200);
        }
        if let Err(e) = write_history(&state, &items).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok":false,"error":e})),
            )
                .into_response();
        }
    }
    item["ok"] = json!(true);
    Json(item).into_response()
}
async fn history_delete(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<HistoryQuery>,
) -> Response {
    let Some(id) = q.id.filter(|s| !s.is_empty()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"id is required"})),
        )
            .into_response();
    };
    let _guard = state.history_lock.lock().await;
    let mut items = read_history(&state).await;
    let before = items.len();
    items.retain(|v| v.get("id").and_then(Value::as_str) != Some(id.as_str()));
    match write_history(&state, &items).await {
        Ok(()) => Json(json!({"ok":true,"removed":items.len()<before,"id":id})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
async fn history_clear(State(state): State<Arc<AppState>>) -> Response {
    let _guard = state.history_lock.lock().await;
    match write_history(&state, &[]).await {
        Ok(()) => Json(json!({"ok":true,"count":0})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
async fn read_history(state: &Arc<AppState>) -> Vec<Value> {
    tokio::fs::read_to_string(state.config.data_dir.join("history.json"))
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
        .unwrap_or_default()
}
async fn write_history(state: &Arc<AppState>, items: &[Value]) -> Result<(), String> {
    atomic_json_write(&state.config.data_dir.join("history.json"), &json!(items)).await
}
async fn get_effort(State(state): State<Arc<AppState>>) -> Response {
    let value = tokio::fs::read_to_string(state.config.data_dir.join("settings.json"))
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(json!({}));
    let effort = value.get("effort").and_then(Value::as_str).unwrap_or("off");
    Json(json!({"ok":true,"effort":effort,"levels":["off","low","high","max"]})).into_response()
}
async fn set_effort(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let Some(effort) = body.get("effort").and_then(Value::as_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"effort is required"})),
        )
            .into_response();
    };
    if !["off", "low", "high", "max", "minimal"].contains(&effort) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"invalid effort level"})),
        )
            .into_response();
    }
    let path = state.config.data_dir.join("settings.json");
    let mut settings = tokio::fs::read_to_string(&path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or(json!({}));
    settings["effort"] = json!(if effort == "minimal" { "low" } else { effort });
    match atomic_json_write(&path, &settings).await {
        Ok(()) => {
            Json(json!({"ok":true,"effort":settings["effort"],"levels":["off","low","high","max"]}))
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
async fn local_models() -> Response {
    Json(json!({"ok":true,"models":[],"available":false,"reason":"Local runtime discovery is not enabled yet; remote models are available."})).into_response()
}
async fn discover_key() -> Response {
    Json(json!({"ok":false,"reason":"not_implemented","message":"For safety, API keys are not discovered or extracted from arbitrary files automatically."})).into_response()
}
async fn chat_mesh(State(state): State<Arc<AppState>>) -> Response {
    let runs = state.runs.lock().await;
    Json(json!({"ok":true,"active_runs":runs.len(),"mesh_enabled":false,"detail":"Distributed orchestration will be provided by the plugin host."})).into_response()
}
async fn notification_capabilities() -> Response {
    Json(json!({"ok":true,"capabilities":["browser_push","sound"],"browser_push":true,"sound":true,"webhook_events":false,"email":false,"available":true,"reason":"Browser alerts depend on the user's HUD notification setting and browser permission; external webhook and email providers are not configured."})).into_response()
}
async fn system_storage(State(state): State<Arc<AppState>>) -> Response {
    let root = &state.config.data_dir;
    let db_size = tokio::fs::metadata(&state.config.database)
        .await
        .ok()
        .map(|m| m.len())
        .unwrap_or(0);
    let uploads = state.config.workspace.join(".harness/uploads");
    let upload_size = directory_size(&uploads).unwrap_or(0);
    Json(json!({"ok":true,"data_dir":root,"database_bytes":db_size,"uploaded_files_bytes":upload_size,"workspace":state.config.workspace,"ram_budget_bytes":1536u64*1024*1024})).into_response()
}
fn directory_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if !path.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let m = entry.metadata()?;
        if m.is_file() {
            total = total.saturating_add(m.len());
        }
    }
    Ok(total)
}
async fn learning_candidate(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let file = state.config.data_dir.join("learning-candidates.json");
    let _guard = state.history_lock.lock().await;
    let mut items = tokio::fs::read_to_string(&file)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
        .unwrap_or_default();
    let id = body
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match body.get("action").and_then(Value::as_str).unwrap_or("add") {
        "remove" => items.retain(|x| x.get("id").and_then(Value::as_str) != Some(id.as_str())),
        "add" => {
            if id.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok":false,"error":"id is required"})),
                )
                    .into_response();
            }
            items.retain(|x| x.get("id").and_then(Value::as_str) != Some(id.as_str()));
            items.push(body.clone());
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok":false,"error":"action must be add or remove"})),
            )
                .into_response();
        }
    }
    match atomic_json_write(&file, &json!(items)).await {
        Ok(()) => Json(json!({"ok":true,"count":items.len()})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
async fn learning_reset(State(state): State<Arc<AppState>>) -> Response {
    match atomic_json_write(
        &state.config.data_dir.join("learning-candidates.json"),
        &json!([]),
    )
    .await
    {
        Ok(()) => Json(json!({"ok":true,"reset":true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
async fn browser_status() -> Response {
    Json(json!({"ok":true,"available":false,"reason":"Browser CDP plugin is not loaded."}))
        .into_response()
}
async fn browser_navigate() -> Response {
    (StatusCode::NOT_IMPLEMENTED,Json(json!({"ok":false,"reason":"not_implemented","message":"Enable the browser plugin to navigate."}))).into_response()
}
async fn browser_input() -> Response {
    (StatusCode::NOT_IMPLEMENTED,Json(json!({"ok":false,"reason":"not_implemented","message":"Enable the browser plugin to interact with pages."}))).into_response()
}
async fn mobile_devices() -> Response {
    Json(json!({"ok":true,"devices":[],"available":false,"reason":"the mobile connector is managed separately from the harness."})).into_response()
}
async fn mobile_command() -> Response {
    (StatusCode::NOT_IMPLEMENTED,Json(json!({"ok":false,"reason":"not_implemented","message":"Mobile commands require the separate Thomas mobile-agent adapter."}))).into_response()
}
async fn mobile_pairing() -> Response {
    (StatusCode::NOT_IMPLEMENTED,Json(json!({"ok":false,"reason":"not_implemented","message":"Pairing is not part of the coding-agent core."}))).into_response()
}
async fn plugin_install(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let id = body
        .get("id")
        .or_else(|| body.get("plugin_id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let action = body
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("install");
    if id.is_empty() || id.len() > 96 || id.contains('/') || id.contains('\\') {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"a safe plugin id is required"})),
        )
            .into_response();
    }
    let exists = discover_plugin_manifests(&state.config.plugins_dir)
        .iter()
        .any(|p| p.get("id").and_then(Value::as_str) == Some(id));
    if !exists {
        return (StatusCode::NOT_FOUND,Json(json!({"ok":false,"reason":"manifest_not_found","message":"Place a reviewed plugin.json or manifest.json under the configured plugins directory before enabling it."}))).into_response();
    }
    let path = state.config.data_dir.join("plugins-enabled.json");
    let mut enabled = tokio::fs::read_to_string(&path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default();
    match action {
        "enable" | "install" => {
            if !enabled.iter().any(|v| v == id) {
                enabled.push(id.to_owned());
            }
        }
        "disable" | "uninstall" => enabled.retain(|v| v != id),
        _ => return (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"ok":false,"error":"action must be enable, disable, install or uninstall"}),
            ),
        )
            .into_response(),
    }
    match atomic_json_write(&path, &json!(enabled)).await {
        Ok(()) => {
            let action_state = if action == "disable" || action == "uninstall" {
                "disabled"
            } else {
                "enabled_manifest_only"
            };
            Json(json!({"ok":true,"id":id,"status":action_state,"execution":"not_started","detail":"Enabling a manifest never executes arbitrary code; a sandboxed MCP/Rust adapter must be configured separately."})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e})),
        )
            .into_response(),
    }
}
#[derive(serde::Deserialize)]
struct DownloadQuery {
    path: String,
}
async fn download_file(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<DownloadQuery>,
) -> Response {
    if q.path.trim().is_empty() || q.path.len() > 4096 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"invalid download path"})),
        )
            .into_response();
    }
    let raw = std::path::PathBuf::from(&q.path);
    let candidate = if raw.is_absolute() {
        raw
    } else {
        state.config.workspace.join(raw)
    };
    let path = match fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"ok":false,"error":"file not found"})),
            )
                .into_response();
        }
    };
    let root = fs::canonicalize(&state.config.workspace)
        .unwrap_or_else(|_| state.config.workspace.clone());
    if !path.starts_with(&root) || !path.is_file() {
        return (StatusCode::FORBIDDEN,Json(json!({"ok":false,"error":"downloads are limited to regular files in the workspace"}))).into_response();
    }
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"ok":false,"error":"file not found"})),
            )
                .into_response();
        }
    };
    if meta.len() > 25 * 1024 * 1024 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"ok":false,"error":"download limit is 25 MiB"})),
        )
            .into_response();
    }
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok":false,"error":"could not read file"})),
            )
                .into_response();
        }
    };
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download.bin");
    let safe_name = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-() ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", safe_name),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn list_uploads(State(state): State<Arc<AppState>>) -> Response {
    let dir = state.config.workspace.join(".harness/uploads");
    let mut uploads = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten().take(300) {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    uploads.push(json!({"name":entry.file_name().to_string_lossy(),"path":entry.path().to_string_lossy(),"size":meta.len(),"modified":meta.modified().ok().and_then(|v|v.duration_since(UNIX_EPOCH).ok()).map(|d|d.as_secs()).unwrap_or(0)}));
                }
            }
        }
    }
    Json(json!({"ok":true,"uploads":uploads,"count":uploads.len()})).into_response()
}
async fn upload_file(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    use base64::Engine as _;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("upload.bin");
    let encoded = body.get("data").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() || name.len() > 240 || encoded.len() > 36 * 1024 * 1024 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"invalid name or file exceeds 25 MiB"})),
        )
            .into_response();
    }
    let clean = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._- ()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let bytes = match base64::engine::general_purpose::STANDARD.decode(encoded) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok":false,"error":"invalid base64 upload"})),
            )
                .into_response();
        }
    };
    if bytes.len() > 25 * 1024 * 1024 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"ok":false,"error":"file exceeds 25 MiB"})),
        )
            .into_response();
    }
    let dir = state.config.workspace.join(".harness/uploads");
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e.to_string()})),
        )
            .into_response();
    }
    let path = dir.join(format!("{}-{}", Uuid::new_v4().simple(), clean));
    let target = path.clone();
    let saved = tokio::task::spawn_blocking(move || atomic_bytes_write(&target, &bytes)).await;
    match saved{
        Ok(Ok(size))=>(StatusCode::OK,Json(json!({"ok":true,"path":path.to_string_lossy(),"name":clean,"type":body.get("type").and_then(Value::as_str).unwrap_or("application/octet-stream"),"size":size}))).into_response(),
        Ok(Err(e))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"ok":false,"error":e}))).into_response(),
        Err(e)=>(StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"ok":false,"error":e.to_string()}))).into_response()
    }
}
fn atomic_bytes_write(path: &FsPath, bytes: &[u8]) -> Result<usize, String> {
    let parent = path.parent().ok_or("path has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    use std::io::Write as _;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.error.to_string())?;
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(bytes.len())
}
async fn upload_delete(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let raw = body
        .get("path")
        .or_else(|| body.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let dir = state.config.workspace.join(".harness/uploads");
    let root = fs::canonicalize(&dir).unwrap_or(dir.clone());
    let path = std::path::PathBuf::from(raw);
    let candidate = if path.is_absolute() {
        fs::canonicalize(&path).ok()
    } else {
        fs::canonicalize(dir.join(path)).ok()
    };
    if candidate
        .as_ref()
        .is_none_or(|p| !p.starts_with(&root) || !p.is_file())
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok":false,"error":"upload path is not allowed"})),
        )
            .into_response();
    }
    match tokio::fs::remove_file(candidate.unwrap()).await {
        Ok(()) => Json(json!({"ok":true,"deleted":true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e.to_string()})),
        )
            .into_response(),
    }
}
async fn workspace_photos(State(state): State<Arc<AppState>>) -> Response {
    let roots = [
        state.config.workspace.join("photos"),
        state.config.workspace.join("images"),
        state.config.workspace.join(".harness/uploads"),
    ];
    let mut photos = Vec::new();
    for root in roots {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten().take(100) {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let ext = p
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
                    photos.push(json!({"name":p.file_name().and_then(|x|x.to_str()).unwrap_or("image"),"path":p.to_string_lossy()}));
                }
            }
        }
    }
    Json(json!({"ok":true,"photos":photos,"count":photos.len()})).into_response()
}
#[derive(serde::Deserialize, Default)]
struct MemoryQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    limit: usize,
}
async fn memory_api(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<MemoryQuery>,
) -> Response {
    let scope = if q.scope.is_empty() {
        "global"
    } else {
        q.scope.as_str()
    };
    let results = if q.q.trim().is_empty() {
        match state.memory.list_memories(q.limit.max(1).min(200)) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"ok":false,"error":e.to_string()})),
                )
                    .into_response();
            }
        }
    } else {
        match state
            .memory
            .search_memory(&q.q, scope, q.limit.max(1).min(50))
        {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok":false,"error":e.to_string()})),
                )
                    .into_response();
            }
        }
    };
    Json(json!({"ok":true,"memories":results,"count":results.len()})).into_response()
}
async fn memory_save(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let text = body
        .get("text")
        .or_else(|| body.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() || text.len() > 4000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"error":"memory text must contain 1–4000 bytes"})),
        )
            .into_response();
    }
    if !crate::memory::is_safe_memory_text(text) {
        return (StatusCode::BAD_REQUEST,Json(json!({"ok":false,"error":"possible secret detected; secrets must be stored in a key manager, not memory"}))).into_response();
    }
    let kind = body.get("kind").and_then(Value::as_str).unwrap_or("fact");
    let scope = body
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("global");
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("user_explicit");
    let importance = body
        .get("importance")
        .and_then(Value::as_f64)
        .unwrap_or(0.8);
    let confidence = body
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.9);
    match state
        .memory
        .save_memory(kind, text, scope, importance, confidence, source)
    {
        Ok(id) => {
            Json(json!({"ok":true,"id":id,"kind":kind,"scope":scope,"text":text})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e.to_string()})),
        )
            .into_response(),
    }
}
async fn memory_delete(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Response {
    match state.memory.forget_memory(id) {
        Ok(removed) => Json(json!({"ok":true,"removed":removed,"id":id})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"error":e.to_string()})),
        )
            .into_response(),
    }
}
