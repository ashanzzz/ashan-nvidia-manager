mod api;
mod crypto;
mod db;
mod error;
mod models;
mod newapi;
mod nvidia;
mod ranking;
mod scheduler;
mod sync;
mod tester;

use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::Router;
use crypto::Crypto;
use db::Database;
use models::SchedulerStatus;
use reqwest::Client;
use tokio::sync::{Mutex, Notify, RwLock};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub crypto: Arc<Crypto>,
    pub http: Client,
    pub sync_guard: Arc<Mutex<()>>,
    pub scheduler_notify: Arc<Notify>,
    pub scheduler_status: Arc<RwLock<SchedulerStatus>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    const INTERNAL_HTTP_PORT: u16 = 8080;
    let data_dir = PathBuf::from(env::var("DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let web_dir = PathBuf::from(env::var("WEB_DIR").unwrap_or_else(|_| "./frontend/dist".into()));
    let master_key = env::var("APP_MASTER_KEY")
        .unwrap_or_else(|_| "ashan-nvidia-manager-default-secret-key-32b".into());

    tokio::fs::create_dir_all(&data_dir).await?;
    let db_path = data_dir.join("nvidia-manager.db");
    let db = Database::connect(&db_path).await?;
    db.init().await?;
    db.recover_interrupted_runs().await?;
    db.ensure_default_settings().await?;

    let http = Client::builder()
        .user_agent("ashan-nvidia-manager/1.0.0")
        .timeout(std::time::Duration::from_secs(45))
        .build()?;

    let state = AppState {
        db,
        crypto: Arc::new(Crypto::new(&master_key)),
        http,
        sync_guard: Arc::new(Mutex::new(())),
        scheduler_notify: Arc::new(Notify::new()),
        scheduler_status: Arc::new(RwLock::new(SchedulerStatus::default())),
    };

    let scheduler_state = state.clone();

    let api_router = api::router(state.clone());
    let index = web_dir.join("index.html");
    let static_service = ServeDir::new(web_dir).fallback(ServeFile::new(index));

    let app = Router::new()
        .merge(api_router)
        .fallback_service(static_service)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], INTERNAL_HTTP_PORT));
    info!(%addr, "Ashan NVIDIA Manager v1.0.0 listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tokio::select! {
        _ = scheduler::run(scheduler_state) => {
            return Err(anyhow::anyhow!("scheduler exited unexpectedly"));
        }
        result = axum::serve(listener, app) => {
            result?;
        }
    }
    Ok(())
}
