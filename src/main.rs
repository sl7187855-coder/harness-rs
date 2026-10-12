use harness_rs::{
    config::Config, cordis_runtime::CordisRuntime, memory::MemoryStore, server, tools::ToolRuntime,
    types::AppState,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow_shim::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let config = Config::from_env();
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(&config.workspace)?;
    std::fs::create_dir_all(&config.skills_dir)?;
    std::fs::create_dir_all(&config.plugins_dir)?;

    let memory = MemoryStore::open(&config.database)?;
    let cordis = CordisRuntime::start().await?;
    let tools = ToolRuntime::new(config.clone(), memory.clone());
    tools.load_worktrees();
    let state = Arc::new(AppState::new(config.clone(), memory, tools, cordis));
    server::load_provider_profiles(&state).await;

    let schedule_state = state.clone();
    tokio::spawn(async move {
        harness_rs::agent::run_schedule_worker(schedule_state).await;
    });

    let addr: SocketAddr = config.bind.parse()?;
    let listener = TcpListener::bind(addr).await?;
    let bridge_bind =
        std::env::var("HARNESS_BRIDGE_BIND").unwrap_or_else(|_| "127.0.0.1:3196".into());
    let bridge_addr: SocketAddr = bridge_bind.parse()?;
    let bridge_listener = TcpListener::bind(bridge_addr).await?;
    let shutdown = CancellationToken::new();
    let bridge_shutdown = shutdown.clone();
    let bridge_state = state.clone();

    tokio::spawn(async move {
        tracing::info!(address=%bridge_addr, "HARNESS-RS compatibility bridge listening");
        if let Err(e) = axum::serve(bridge_listener, server::bridge_router(bridge_state))
            .with_graceful_shutdown(async move {
                bridge_shutdown.cancelled().await;
            })
            .await
        {
            tracing::error!(error=%e, "bridge listener failed");
        }
    });

    tracing::info!(address=%addr, "HARNESS-RS HUD gateway listening");
    tracing::info!(
        tool_count = harness_rs::tool_catalog::all().len(),
        tool_ready = tool_catalog_count_ready(),
        "Claude Code core catalogue and Harness extensions loaded"
    );
    axum::serve(listener, server::router(state))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown requested");
            shutdown.cancel();
        })
        .await?;
    Ok(())
}

fn tool_catalog_count_ready() -> usize {
    harness_rs::tool_catalog::all()
        .iter()
        .filter(|t| t.ready())
        .count()
}

// Keep main's error type small and compatible with the host's minimal install.
mod anyhow_shim {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
}
