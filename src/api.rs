use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{AppSettings, NvidiaAccount, NvidiaAccountPayload, NvidiaModelItem, SyncRun},
    nvidia::NvidiaClient,
    ranking::select_and_rank_models,
    sync::run_sync,
    tester::ModelTester,
    AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/accounts", get(list_accounts).post(save_account))
        .route("/api/accounts/{id}", delete(delete_account))
        .route("/api/accounts/{id}/probe", post(probe_single_account))
        .route("/api/models", get(list_models))
        .route("/api/settings", get(get_settings).post(save_settings))
        .route("/api/sync/start", post(start_sync))
        .route("/api/sync/runs", get(list_sync_runs))
        .route("/api/sync/{id}", get(get_sync_run))
        .route("/api/sync/{id}/logs", get(get_sync_logs))
        .with_state(state)
}

async fn get_status(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let accounts = state.db.list_accounts().await?;
    let total_accounts = accounts.len();
    let healthy_accounts = accounts.iter().filter(|a| a.status == "healthy" && a.enabled).count();
    let runs = state.db.list_sync_runs(1).await?;
    let last_run = runs.first().cloned();
    let scheduler_status = state.scheduler_status.read().await.clone();

    Ok(Json(json!({
        "status": "ok",
        "service": "ashan-nvidia-manager",
        "version": env!("CARGO_PKG_VERSION"),
        "total_accounts": total_accounts,
        "healthy_accounts": healthy_accounts,
        "last_run": last_run,
        "scheduler": scheduler_status,
    })))
}

async fn list_accounts(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let mut accounts = state.db.list_accounts().await?;
    for a in &mut accounts {
        if a.api_key.len() > 12 {
            let prefix = &a.api_key[..6];
            let suffix = &a.api_key[a.api_key.len() - 4..];
            a.api_key = format!("{}****{}", prefix, suffix);
        } else {
            a.api_key = "********".into();
        }
    }
    Ok(Json(accounts))
}

async fn save_account(
    State(state): State<AppState>,
    Json(payload): Json<NvidiaAccountPayload>,
) -> Result<impl IntoResponse, AppError> {
    let accounts = state.db.list_accounts().await?;
    let existing = accounts.iter().find(|a| a.name == payload.name);

    let id = existing.map(|a| a.id.clone()).unwrap_or_else(|| Uuid::new_v4().to_string());
    let priority = payload.priority.unwrap_or(0);
    let weight = payload.weight.unwrap_or(0);
    let enabled = payload.enabled.unwrap_or(true);

    let encrypted_key = if let Some(raw_key) = payload.api_key {
        if !raw_key.trim().is_empty() && !raw_key.contains("****") {
            state.crypto.encrypt(raw_key.trim())?
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    state.db.upsert_account(
        &id,
        &payload.name,
        &encrypted_key,
        payload.newapi_channel_id,
        priority,
        weight,
        enabled,
    ).await?;

    Ok(Json(json!({"status": "saved", "id": id})))
}

async fn delete_account(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.db.delete_account(&id).await?;
    Ok(Json(json!({"status": "deleted"})))
}

async fn probe_single_account(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let account = state.db.get_account(&id).await?.ok_or_else(|| AppError::bad("账号不存在"))?;
    let raw_key = match state.crypto.decrypt(&account.api_key) {
        Ok(k) if !k.is_empty() => k,
        _ => account.api_key.clone(),
    };

    let settings = state.db.get_settings().await?;
    let client = NvidiaClient::new(state.http.clone(), &settings.nvidia_api_base);
    let tester = ModelTester::new(client, settings.clone());

    let probe_model = "nvidia/nemotron-3.5-super-vl-120b-a12b";
    let probe_res = tester.probe_account(&account, &raw_key, probe_model).await;
    state.db.update_account_probe(&account.id, &probe_res.status, &probe_res.detail).await?;

    Ok(Json(json!({
        "status": probe_res.status,
        "is_healthy": probe_res.is_healthy,
        "detail": probe_res.detail,
        "model_latencies": probe_res.model_latencies,
    })))
}

async fn list_models(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let settings = state.db.get_settings().await?;
    let candidates = select_and_rank_models(&[], &settings);
    Ok(Json(candidates))
}

async fn get_settings(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let settings = state.db.get_settings().await?;
    Ok(Json(settings))
}

async fn save_settings(
    State(state): State<AppState>,
    Json(settings): Json<AppSettings>,
) -> Result<impl IntoResponse, AppError> {
    state.db.save_settings(&settings).await?;
    state.scheduler_notify.notify_one();
    Ok(Json(json!({"status": "saved"})))
}

async fn start_sync(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let db = state.db.clone();
    let crypto = state.crypto.clone();
    let http = state.http.clone();
    let sync_guard = state.sync_guard.clone();
    let run_id = Uuid::new_v4().to_string();
    let run_id_clone = run_id.clone();

    tokio::spawn(async move {
        if let Err(e) = run_sync(db, crypto, http, sync_guard, "manual_web").await {
            tracing::error!("手动同步任务失败: {e}");
        }
    });

    Ok(Json(json!({"status": "started", "run_id": run_id_clone})))
}

async fn list_sync_runs(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let runs = state.db.list_sync_runs(20).await?;
    Ok(Json(runs))
}

async fn get_sync_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let run = state.db.get_sync_run(&id).await?.ok_or_else(|| AppError::bad("任务不存在"))?;
    Ok(Json(run))
}

async fn get_sync_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let logs = state.db.get_sync_logs(&id).await?;
    Ok(Json(logs))
}
