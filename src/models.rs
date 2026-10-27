use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub nvidia_api_base: String,
    pub newapi_base_url: String,
    pub newapi_admin_user_id: String,
    pub alias_model: String,
    #[serde(default = "default_true")]
    pub expose_upstream_models: bool,
    pub managed_group: String,
    #[serde(default = "default_routing_groups")]
    pub routing_groups: Vec<String>,
    pub managed_tag: String,
    pub channel_name_prefix: String,
    pub min_context_length: i64,
    pub candidate_pool: usize,
    pub preflight_concurrency: usize,
    #[serde(default = "default_health_check_attempts")]
    pub health_check_attempts: usize,
    #[serde(default = "default_health_check_interval_seconds")]
    pub health_check_interval_seconds: u64,
    #[serde(default = "default_health_min_success_rate")]
    pub health_min_success_rate: f64,
    pub include_models: Vec<String>,
    pub exclude_models: Vec<String>,
    pub auto_sync: bool,
    #[serde(default)]
    pub schedule_mode: ScheduleMode,
    pub sync_interval_minutes: u64,
    #[serde(default = "default_daily_sync_time")]
    pub daily_sync_time: String,
    #[serde(default = "default_schedule_timezone")]
    pub schedule_timezone: String,
    pub channel_type: i64, // 1 for OpenAI-compatible
    pub auto_ban: bool,
    pub e2e_test_enabled: bool,
}

fn default_true() -> bool { true }
fn default_health_check_attempts() -> usize { 3 }
fn default_health_check_interval_seconds() -> u64 { 15 }
fn default_health_min_success_rate() -> f64 { 0.8 }
fn default_routing_groups() -> Vec<String> { vec!["default".into()] }
fn default_daily_sync_time() -> String { "04:00".into() }
fn default_schedule_timezone() -> String { "Asia/Shanghai".into() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            nvidia_api_base: "https://integrate.api.nvidia.com/v1".into(),
            newapi_base_url: "http://192.168.8.11:3001".into(),
            newapi_admin_user_id: "1".into(),
            alias_model: "ashan-ai-model-normal".into(),
            expose_upstream_models: true,
            managed_group: "ashan-nvidia-managed".into(),
            routing_groups: default_routing_groups(),
            managed_tag: "ashan-nvidia-manager".into(),
            channel_name_prefix: "[ANM]".into(),
            min_context_length: 32768,
            candidate_pool: 10,
            preflight_concurrency: 4,
            health_check_attempts: 3,
            health_check_interval_seconds: 15,
            health_min_success_rate: 0.8,
            include_models: vec![],
            exclude_models: vec![],
            auto_sync: false,
            schedule_mode: ScheduleMode::Interval,
            sync_interval_minutes: 360,
            daily_sync_time: default_daily_sync_time(),
            schedule_timezone: default_schedule_timezone(),
            channel_type: 1, // OpenAI standard
            auto_ban: true,
            e2e_test_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMode {
    #[default]
    Interval,
    Daily,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvidiaAccount {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub newapi_channel_id: Option<i64>,
    pub priority: i64,
    pub weight: i64,
    pub enabled: bool,
    pub status: String, // "healthy", "error", "rate_limited", "unknown"
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvidiaAccountPayload {
    pub name: String,
    pub api_key: Option<String>,
    pub newapi_channel_id: Option<i64>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvidiaModelItem {
    pub id: String,
    pub name: String,
    pub architecture: String,
    pub context_length: i64,
    pub max_output_tokens: i64,
    pub category: String, // "reasoning", "multimodal", "fast", "dense"
    pub total_parameters: String,
    pub active_parameters: String,
    pub benchmark_swe: Option<f64>,
    pub benchmark_mmlu: Option<f64>,
    pub benchmark_terminal: Option<f64>,
    pub is_online: bool,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRun {
    pub id: String,
    pub status: String, // "running", "success", "failed"
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub message: String,
    pub candidate_models: Vec<String>,
    pub updated_channels: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRunLog {
    pub id: i64,
    pub run_id: String,
    pub level: String,
    pub stage: String,
    pub message: String,
    pub detail: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerStatus {
    pub is_running: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}
