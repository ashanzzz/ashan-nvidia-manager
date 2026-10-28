use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    crypto::Crypto,
    db::Database,
    error::AppError,
    models::NvidiaModelItem,
    newapi::NewApiClient,
    nvidia::NvidiaClient,
    ranking::select_and_rank_models,
    tester::ModelTester,
};

pub async fn run_sync(
    db: Database,
    crypto: Arc<Crypto>,
    http: reqwest::Client,
    sync_guard: Arc<Mutex<()>>,
    trigger: &str,
    preset_run_id: Option<String>,
) -> Result<String, AppError> {
    let _permit = match sync_guard.try_lock() {
        Ok(guard) => guard,
        Err(_) => return Err(AppError::bad("已有同步任务正在进行中，请稍后再试")),
    };

    let run_id = preset_run_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    db.start_sync_run(&run_id, trigger).await?;
    db.append_log(&run_id, "info", "init", &format!("同步任务已启动 (触发来源: {trigger})"), None).await?;

    let settings = match db.get_settings().await {
        Ok(s) => s,
        Err(e) => {
            let err = format!("获取配置失败: {e}");
            db.append_log(&run_id, "error", "init", &err, None).await?;
            db.finish_sync_run(&run_id, "failed", &[], &[], Some(&err)).await?;
            return Err(AppError::bad(err));
        }
    };

    let accounts = match db.list_accounts().await {
        Ok(accs) => accs,
        Err(e) => {
            let err = format!("读取账号列表失败: {e}");
            db.append_log(&run_id, "error", "init", &err, None).await?;
            db.finish_sync_run(&run_id, "failed", &[], &[], Some(&err)).await?;
            return Err(AppError::bad(err));
        }
    };

    if accounts.is_empty() {
        let msg = "未配置任何 NVIDIA 账号，请在账号管理页面添加 API Key";
        db.append_log(&run_id, "warn", "init", msg, None).await?;
        db.finish_sync_run(&run_id, "success", &[], &[], Some(msg)).await?;
        return Ok(run_id);
    }

    db.append_log(
        &run_id,
        "info",
        "accounts",
        &format!("已加载 {} 个 NVIDIA 账号，准备进行可用性探测", accounts.len()),
        None,
    )
    .await?;

    let newapi_token = db.get_secret("newapi_admin_token").await?.unwrap_or_default();
    let newapi_token = if !newapi_token.is_empty() {
        crypto.decrypt(&newapi_token).unwrap_or(newapi_token)
    } else {
        String::new()
    };

    let newapi_client = if !settings.newapi_base_url.is_empty() && !newapi_token.is_empty() {
        match NewApiClient::new(http.clone(), &settings, newapi_token) {
            Ok(cli) => Some(cli),
            Err(e) => {
                db.append_log(&run_id, "warn", "newapi", &format!("New API 客户端初始化失败: {e}"), None).await?;
                None
            }
        }
    } else {
        db.append_log(&run_id, "warn", "newapi", "未配置有效的 New API Base URL 或 Admin Token，跳过渠道同步", None).await?;
        None
    };

    let nvidia_client = NvidiaClient::new(http.clone(), &settings.nvidia_api_base);
    let tester = ModelTester::new(nvidia_client.clone(), settings.clone());

    // Step 1: Discover live models using first available key
    let mut live_model_ids = Vec::new();
    let mut probe_key = String::new();
    for acc in &accounts {
        if !acc.enabled { continue; }
        let raw_key = match crypto.decrypt(&acc.api_key) {
            Ok(k) if !k.is_empty() => k,
            _ => acc.api_key.clone(),
        };
        if let Ok(ids) = nvidia_client.list_models(&raw_key).await {
            live_model_ids = ids;
            probe_key = raw_key;
            break;
        }
    }

    let initial_candidates = select_and_rank_models(&live_model_ids, &settings);

    // Preflight candidate models to filter out models that are currently timing out or offline upstream
    let candidate_models = if !probe_key.is_empty() && !initial_candidates.is_empty() {
        let test_results = tester.test_candidates(&probe_key, &initial_candidates).await;
        let responsive_ids: std::collections::HashSet<String> = test_results
            .into_iter()
            .filter(|(_, ok, _, _)| *ok)
            .map(|(id, _, _, _)| id)
            .collect();
        let filtered: Vec<NvidiaModelItem> = initial_candidates
            .into_iter()
            .filter(|m| responsive_ids.contains(&m.id))
            .collect();
        if filtered.is_empty() {
            // fallback if all preflights timed out
            select_and_rank_models(&live_model_ids, &settings)
        } else {
            filtered
        }
    } else {
        initial_candidates
    };

    let candidate_ids: Vec<String> = candidate_models.iter().map(|m| m.id.clone()).collect();

    db.append_log(
        &run_id,
        "info",
        "catalog",
        &format!("模型优选完成，筛选出 {} 个健康主力模型 (按智力从高到低排序)", candidate_models.len()),
        Some(&candidate_ids.join(", ")),
    )
    .await?;

    let top_tier_count = std::cmp::min(3, candidate_ids.len());

    // Step 2: Probe all accounts & sync channels with tiered intelligence mapping
    let mut updated_channels = Vec::new();
    let mut healthy_count = 0;

    for (idx, acc) in accounts.iter().enumerate() {
        let tier_idx = if accounts.is_empty() || top_tier_count == 0 {
            0
        } else {
            (idx * top_tier_count) / accounts.len()
        };
        let target_model = candidate_ids
            .get(tier_idx)
            .cloned()
            .unwrap_or_else(|| "nvidia/nemotron-3.5-super-vl-120b-a12b".into());
        let tier_label = format!("第 {} 梯队 (Top {})", tier_idx + 1, tier_idx + 1);

        let raw_key = match crypto.decrypt(&acc.api_key) {
            Ok(k) if !k.is_empty() => k,
            _ => acc.api_key.clone(),
        };

        db.append_log(
            &run_id,
            "info",
            "probe",
            &format!(
                "探测账号 [{}] ({} / 优先级: {}, 权重: {}, 绑定模型: {})",
                acc.name, tier_label, acc.priority, acc.weight, target_model
            ),
            None,
        )
        .await?;

        let probe_res = tester.probe_account(acc, &raw_key, &target_model).await;
        db.update_account_probe(&acc.id, &probe_res.status, &probe_res.detail).await?;

        if probe_res.is_healthy {
            healthy_count += 1;
            db.append_log(
                &run_id,
                "info",
                "probe",
                &format!("账号 [{}] 正常在线 ({})", acc.name, probe_res.detail),
                None,
            )
            .await?;

            let mut active_acc = acc.clone();
            active_acc.status = probe_res.status.clone();

            if let Some(ref newapi) = newapi_client {
                match newapi
                    .sync_account_channel(&active_acc, &raw_key, &candidate_ids, &settings, &target_model)
                    .await
                {
                    Ok(channel_id) => {
                        db.update_account_channel_id(&acc.id, channel_id).await?;
                        updated_channels.push(channel_id);
                        db.append_log(
                            &run_id,
                            "info",
                            "newapi",
                            &format!(
                                "成功同步账号 [{}] 到 New API 渠道 ID {} ({} 映射: {} -> {})",
                                acc.name, channel_id, tier_label, settings.alias_model, target_model
                            ),
                            None,
                        )
                        .await?;
                    }
                    Err(e) => {
                        db.append_log(
                            &run_id,
                            "error",
                            "newapi",
                            &format!("同步账号 [{}] 到 New API 失败: {e}", acc.name),
                            None,
                        )
                        .await?;
                    }
                }
            }
        } else {
            db.append_log(
                &run_id,
                "warn",
                "probe",
                &format!("账号 [{}] 探测异常: {}", acc.name, probe_res.detail),
                None,
            )
            .await?;

            // If account is disabled or unhealthy, disable its New API channel
            if let Some(ref newapi) = newapi_client {
                if let Some(channel_id) = acc.newapi_channel_id {
                    let mut disabled_acc = acc.clone();
                    disabled_acc.enabled = false;
                    let _ = newapi
                        .sync_account_channel(&disabled_acc, &raw_key, &candidate_ids, &settings, &target_model)
                        .await;
                }
            }
        }
    }

    let summary_msg = format!(
        "同步完成：总账号 {} 个，健康在线 {} 个，同步 New API 渠道 {} 个",
        accounts.len(),
        healthy_count,
        updated_channels.len()
    );

    db.append_log(&run_id, "info", "complete", &summary_msg, None).await?;
    db.finish_sync_run(&run_id, "success", &candidate_ids, &updated_channels, None).await?;

    Ok(run_id)
}
