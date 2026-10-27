use std::{path::Path, str::FromStr};

use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::Tz;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};

use crate::{
    error::AppError,
    models::{AppSettings, NvidiaAccount, ScheduleMode, SyncRun, SyncRunLog},
};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn connect(path: &Path) -> anyhow::Result<Self> {
        let url = format!("sqlite://{}", path.to_string_lossy());
        let options = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Ok(Self { pool })
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS secrets (
                name TEXT PRIMARY KEY,
                ciphertext TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS nvidia_accounts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                encrypted_api_key TEXT NOT NULL,
                newapi_channel_id INTEGER,
                priority INTEGER NOT NULL DEFAULT 0,
                weight INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'unknown',
                last_probe_at TEXT,
                last_probe_status TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sync_runs (
                id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                trigger TEXT NOT NULL,
                status TEXT NOT NULL,
                changed INTEGER NOT NULL,
                candidate_models TEXT NOT NULL,
                error TEXT
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sync_run_logs (
                run_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                level TEXT NOT NULL,
                stage TEXT NOT NULL,
                message TEXT NOT NULL,
                detail TEXT,
                PRIMARY KEY(run_id, seq)
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sync_run_logs_run_id ON sync_run_logs(run_id, seq)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn recover_interrupted_runs(&self) -> Result<(), AppError> {
        let rows = sqlx::query("SELECT id FROM sync_runs WHERE status='running'")
            .fetch_all(&self.pool)
            .await?;
        if rows.is_empty() {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        for row in rows {
            let id: String = row.get("id");
            sqlx::query(
                "UPDATE sync_runs SET status='failed', ended_at=?, error=? WHERE id=? AND status='running'",
            )
            .bind(&now)
            .bind("服务重启导致同步中断，已自动标记失败")
            .bind(&id)
            .execute(&self.pool)
            .await?;

            let next_seq: i64 = sqlx::query(
                "SELECT COALESCE(MAX(seq),0)+1 AS next_seq FROM sync_run_logs WHERE run_id=?",
            )
            .bind(&id)
            .fetch_one(&self.pool)
            .await?
            .get("next_seq");

            sqlx::query(
                "INSERT OR REPLACE INTO sync_run_logs(run_id,seq,timestamp,level,stage,message,detail) VALUES(?,?,?,?,?,?,?)",
            )
            .bind(&id)
            .bind(next_seq)
            .bind(&now)
            .bind("error")
            .bind("interrupted")
            .bind("任务因容器重启或中断而停止")
            .bind("已自动恢复，可以手动发起重新同步")
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn ensure_default_settings(&self) -> anyhow::Result<()> {
        if self.get_kv("settings").await?.is_none() {
            self.save_settings(&AppSettings::default()).await?;
        }
        Ok(())
    }

    pub async fn get_kv(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM kv WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    pub async fn set_kv(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO kv(key,value,updated_at) VALUES(?,?,?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_secret(&self, name: &str) -> Result<Option<String>, AppError> {
        let row = sqlx::query("SELECT ciphertext FROM secrets WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("ciphertext")))
    }

    pub async fn set_secret(&self, name: &str, ciphertext: &str) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO secrets(name,ciphertext,updated_at) VALUES(?,?,?)
             ON CONFLICT(name) DO UPDATE SET ciphertext=excluded.ciphertext, updated_at=excluded.updated_at",
        )
        .bind(name)
        .bind(ciphertext)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_settings(&self) -> Result<AppSettings, AppError> {
        match self.get_kv("settings").await? {
            Some(v) => Ok(serde_json::from_str(&v)?),
            None => Ok(AppSettings::default()),
        }
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        if settings.alias_model.trim().is_empty() {
            return Err(AppError::bad("alias_model 不能为空"));
        }
        match settings.schedule_mode {
            ScheduleMode::Interval => {
                if settings.sync_interval_minutes < 10 {
                    return Err(AppError::bad("同步间隔不能小于 10 分钟"));
                }
            }
            ScheduleMode::Daily => {
                NaiveTime::parse_from_str(settings.daily_sync_time.trim(), "%H:%M")
                    .map_err(|_| AppError::bad("每日同步时间必须是 HH:MM 格式"))?;
                settings
                    .schedule_timezone
                    .trim()
                    .parse::<Tz>()
                    .map_err(|_| AppError::bad("无效的 IANA 时区"))?;
            }
        }
        self.set_kv("settings", &serde_json::to_string(settings)?).await?;
        Ok(())
    }

    // Account CRUD
    pub async fn list_accounts(&self) -> Result<Vec<NvidiaAccount>, AppError> {
        let rows = sqlx::query(
            "SELECT id, name, encrypted_api_key, newapi_channel_id, priority, weight, enabled, status, last_probe_at, last_probe_status, created_at, updated_at
             FROM nvidia_accounts ORDER BY priority DESC, weight DESC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut accounts = Vec::new();
        for r in rows {
            let last_probe_at: Option<String> = r.get("last_probe_at");
            let created_at: String = r.get("created_at");
            let updated_at: String = r.get("updated_at");

            accounts.push(NvidiaAccount {
                id: r.get("id"),
                name: r.get("name"),
                api_key: r.get("encrypted_api_key"),
                newapi_channel_id: r.get("newapi_channel_id"),
                priority: r.get("priority"),
                weight: r.get("weight"),
                enabled: r.get::<i64, _>("enabled") == 1,
                status: r.get("status"),
                last_probe_at: last_probe_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                last_probe_status: r.get("last_probe_status"),
                created_at: DateTime::parse_from_rfc3339(&created_at).unwrap_or_default().with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at).unwrap_or_default().with_timezone(&Utc),
            });
        }
        Ok(accounts)
    }

    pub async fn get_account(&self, id: &str) -> Result<Option<NvidiaAccount>, AppError> {
        let row = sqlx::query(
            "SELECT id, name, encrypted_api_key, newapi_channel_id, priority, weight, enabled, status, last_probe_at, last_probe_status, created_at, updated_at
             FROM nvidia_accounts WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            let last_probe_at: Option<String> = r.get("last_probe_at");
            let created_at: String = r.get("created_at");
            let updated_at: String = r.get("updated_at");

            Ok(Some(NvidiaAccount {
                id: r.get("id"),
                name: r.get("name"),
                api_key: r.get("encrypted_api_key"),
                newapi_channel_id: r.get("newapi_channel_id"),
                priority: r.get("priority"),
                weight: r.get("weight"),
                enabled: r.get::<i64, _>("enabled") == 1,
                status: r.get("status"),
                last_probe_at: last_probe_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                last_probe_status: r.get("last_probe_status"),
                created_at: DateTime::parse_from_rfc3339(&created_at).unwrap_or_default().with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at).unwrap_or_default().with_timezone(&Utc),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert_account(
        &self,
        id: &str,
        name: &str,
        encrypted_key: &str,
        channel_id: Option<i64>,
        priority: i64,
        weight: i64,
        enabled: bool,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let enabled_int = if enabled { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO nvidia_accounts (id, name, encrypted_api_key, newapi_channel_id, priority, weight, enabled, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'unknown', ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                encrypted_api_key = CASE WHEN excluded.encrypted_api_key != '' THEN excluded.encrypted_api_key ELSE nvidia_accounts.encrypted_api_key END,
                newapi_channel_id = COALESCE(excluded.newapi_channel_id, nvidia_accounts.newapi_channel_id),
                priority = excluded.priority,
                weight = excluded.weight,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(encrypted_key)
        .bind(channel_id)
        .bind(priority)
        .bind(weight)
        .bind(enabled_int)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_account(&self, id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM nvidia_accounts WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_account_probe(
        &self,
        id: &str,
        status: &str,
        probe_status: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE nvidia_accounts SET status = ?, last_probe_status = ?, last_probe_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(probe_status)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_account_channel_id(&self, id: &str, channel_id: i64) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE nvidia_accounts SET newapi_channel_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(channel_id)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // Sync Run methods
    pub async fn start_sync_run(&self, run_id: &str, trigger: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO sync_runs (id, started_at, trigger, status, changed, candidate_models) VALUES (?, ?, ?, 'running', 0, '[]')",
        )
        .bind(run_id)
        .bind(&now)
        .bind(trigger)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn finish_sync_run(
        &self,
        run_id: &str,
        status: &str,
        models: &[String],
        channels: &[i64],
        error: Option<&str>,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let models_json = serde_json::to_string(models)?;
        let changed = channels.len() as i64;
        sqlx::query(
            "UPDATE sync_runs SET status = ?, ended_at = ?, changed = ?, candidate_models = ?, error = ? WHERE id = ?",
        )
        .bind(status)
        .bind(&now)
        .bind(changed)
        .bind(models_json)
        .bind(error)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn append_log(
        &self,
        run_id: &str,
        level: &str,
        stage: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let next_seq: i64 = sqlx::query(
            "SELECT COALESCE(MAX(seq), 0) + 1 AS next_seq FROM sync_run_logs WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await?
        .get("next_seq");

        sqlx::query(
            "INSERT INTO sync_run_logs (run_id, seq, timestamp, level, stage, message, detail) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(next_seq)
        .bind(&now)
        .bind(level)
        .bind(stage)
        .bind(message)
        .bind(detail)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_sync_run(&self, id: &str) -> Result<Option<SyncRun>, AppError> {
        let row = sqlx::query("SELECT id, started_at, ended_at, status, candidate_models, error FROM sync_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(r) = row {
            let started_at: String = r.get("started_at");
            let ended_at: Option<String> = r.get("ended_at");
            let candidate_models_json: String = r.get("candidate_models");
            let candidate_models: Vec<String> = serde_json::from_str(&candidate_models_json).unwrap_or_default();
            let error: Option<String> = r.get("error");

            Ok(Some(SyncRun {
                id: r.get("id"),
                status: r.get("status"),
                started_at: DateTime::parse_from_rfc3339(&started_at).unwrap_or_default().with_timezone(&Utc),
                finished_at: ended_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                message: error.unwrap_or_default(),
                candidate_models,
                updated_channels: vec![],
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_sync_logs(&self, run_id: &str) -> Result<Vec<SyncRunLog>, AppError> {
        let rows = sqlx::query(
            "SELECT seq, timestamp, level, stage, message, detail FROM sync_run_logs WHERE run_id = ? ORDER BY seq ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        let mut logs = Vec::new();
        for r in rows {
            let ts: String = r.get("timestamp");
            logs.push(SyncRunLog {
                id: r.get("seq"),
                run_id: run_id.to_string(),
                level: r.get("level"),
                stage: r.get("stage"),
                message: r.get("message"),
                detail: r.get("detail"),
                created_at: DateTime::parse_from_rfc3339(&ts).unwrap_or_default().with_timezone(&Utc),
            });
        }
        Ok(logs)
    }

    pub async fn list_sync_runs(&self, limit: i64) -> Result<Vec<SyncRun>, AppError> {
        let rows = sqlx::query(
            "SELECT id, started_at, ended_at, status, candidate_models, error FROM sync_runs ORDER BY started_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut runs = Vec::new();
        for r in rows {
            let started_at: String = r.get("started_at");
            let ended_at: Option<String> = r.get("ended_at");
            let candidate_models_json: String = r.get("candidate_models");
            let candidate_models: Vec<String> = serde_json::from_str(&candidate_models_json).unwrap_or_default();
            let error: Option<String> = r.get("error");

            runs.push(SyncRun {
                id: r.get("id"),
                status: r.get("status"),
                started_at: DateTime::parse_from_rfc3339(&started_at).unwrap_or_default().with_timezone(&Utc),
                finished_at: ended_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                message: error.unwrap_or_default(),
                candidate_models,
                updated_channels: vec![],
            });
        }
        Ok(runs)
    }
}
