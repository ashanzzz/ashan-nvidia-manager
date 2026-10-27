use std::time::Duration;
use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::Tz;
use tokio::time::sleep;
use tracing::{error, info};

use crate::{
    models::{AppSettings, ScheduleMode, SchedulerStatus},
    sync::run_sync,
    AppState,
};

pub async fn run(state: AppState) {
    info!("Ashan NVIDIA Manager 调度器已启动");

    loop {
        let settings = match state.db.get_settings().await {
            Ok(s) => s,
            Err(e) => {
                error!("调度器读取配置失败: {e}");
                sleep(Duration::from_secs(30)).await;
                continue;
            }
        };

        if !settings.auto_sync {
            {
                let mut status = state.scheduler_status.write().await;
                status.is_running = false;
                status.next_run_at = None;
            }
            // Wait for settings change notification
            state.scheduler_notify.notified().await;
            continue;
        }

        let next_run = compute_next_run(&settings);
        {
            let mut status = state.scheduler_status.write().await;
            status.is_running = true;
            status.next_run_at = Some(next_run);
        }

        let now = Utc::now();
        let wait_duration = if next_run > now {
            (next_run - now).to_std().unwrap_or(Duration::from_secs(60))
        } else {
            Duration::from_secs(1)
        };

        info!("下次自动同步将于 {} 执行 (等待 {} 秒)", next_run.to_rfc3339(), wait_duration.as_secs());

        tokio::select! {
            _ = sleep(wait_duration) => {
                info!("触发计划自动同步");
                {
                    let mut status = state.scheduler_status.write().await;
                    status.last_run_at = Some(Utc::now());
                }
                let db = state.db.clone();
                let crypto = state.crypto.clone();
                let http = state.http.clone();
                let guard = state.sync_guard.clone();
                if let Err(e) = run_sync(db, crypto, http, guard, "scheduled_cron").await {
                    error!("自动同步执行失败: {e}");
                }
            }
            _ = state.scheduler_notify.notified() => {
                info!("接收到配置更新通知，重新计算调度周期");
            }
        }
    }
}

fn compute_next_run(settings: &AppSettings) -> DateTime<Utc> {
    let now = Utc::now();
    match settings.schedule_mode {
        ScheduleMode::Interval => {
            let interval = Duration::from_secs(settings.sync_interval_minutes.max(10) * 60);
            now + chrono::Duration::from_std(interval).unwrap_or(chrono::Duration::hours(6))
        }
        ScheduleMode::Daily => {
            let tz: Tz = settings.schedule_timezone.parse().unwrap_or(chrono_tz::Asia::Shanghai);
            let target_time = NaiveTime::parse_from_str(&settings.daily_sync_time, "%H:%M")
                .unwrap_or_else(|_| NaiveTime::from_hms_opt(4, 0, 0).unwrap());

            let now_local = now.with_timezone(&tz);
            let mut candidate_local = now_local.date_naive().and_time(target_time);
            if candidate_local <= now_local.naive_local() {
                candidate_local += chrono::Duration::days(1);
            }
            candidate_local.and_local_timezone(tz).earliest().unwrap_or(now_local).with_timezone(&Utc)
        }
    }
}
