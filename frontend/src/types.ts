export type ScheduleMode = 'interval' | 'daily'

export interface Settings {
  nvidia_api_base: string
  newapi_base_url: string
  newapi_admin_user_id: string
  alias_model: string
  expose_upstream_models: boolean
  managed_group: string
  routing_groups: string[]
  managed_tag: string
  channel_name_prefix: string
  min_context_length: number
  candidate_pool: number
  preflight_concurrency: number
  health_check_attempts: number
  health_check_interval_seconds: number
  health_min_success_rate: number
  include_models: string[]
  exclude_models: string[]
  auto_sync: boolean
  schedule_mode: ScheduleMode
  sync_interval_minutes: number
  daily_sync_time: string
  schedule_timezone: string
  channel_type: number
  auto_ban: boolean
  e2e_test_enabled: boolean
}

export interface NvidiaAccount {
  id: string
  name: string
  api_key: string
  newapi_channel_id?: number | null
  priority: number
  weight: number
  enabled: boolean
  status: 'healthy' | 'error' | 'rate_limited' | 'unauthorized' | 'unknown' | string
  last_probe_at?: string | null
  last_probe_status?: string | null
  created_at: string
  updated_at: string
}

export interface NvidiaAccountPayload {
  name: string
  api_key?: string
  newapi_channel_id?: number | null
  priority?: number
  weight?: number
  enabled?: boolean
}

export interface NvidiaModelItem {
  id: string
  name: string
  architecture: string
  context_length: number
  max_output_tokens: number
  category: 'reasoning' | 'multimodal' | 'fast' | 'dense' | string
  total_parameters: string
  active_parameters: string
  benchmark_swe?: number | null
  benchmark_mmlu?: number | null
  benchmark_terminal?: number | null
  is_online: boolean
  latency_ms?: number | null
}

export interface SyncRun {
  id: string
  status: 'running' | 'success' | 'failed' | string
  started_at: string
  finished_at?: string | null
  message: string
  candidate_models: string[]
  updated_channels: number[]
}

export interface SyncRunLog {
  id: number
  run_id: string
  level: 'info' | 'warn' | 'error' | string
  stage: string
  message: string
  detail?: string | null
  created_at: string
}

export interface SystemStatus {
  status: string
  service: string
  version: string
  total_accounts: number
  healthy_accounts: number
  last_run?: SyncRun | null
  scheduler?: {
    is_running: boolean
    last_run_at?: string | null
    next_run_at?: string | null
  }
}
