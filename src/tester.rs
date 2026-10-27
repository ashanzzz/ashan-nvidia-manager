use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;

use crate::{
    error::AppError,
    models::{AppSettings, NvidiaAccount, NvidiaModelItem},
    nvidia::NvidiaClient,
};

#[derive(Debug, Clone)]
pub struct AccountProbeResult {
    pub account_id: String,
    pub account_name: String,
    pub is_healthy: bool,
    pub status: String,
    pub detail: String,
    pub model_latencies: Vec<(String, u64)>,
}

pub struct ModelTester {
    client: NvidiaClient,
    settings: AppSettings,
}

impl ModelTester {
    pub fn new(client: NvidiaClient, settings: AppSettings) -> Self {
        Self { client, settings }
    }

    pub async fn probe_account(
        &self,
        account: &NvidiaAccount,
        plaintext_key: &str,
        test_model: &str,
    ) -> AccountProbeResult {
        let mut result = AccountProbeResult {
            account_id: account.id.clone(),
            account_name: account.name.clone(),
            is_healthy: false,
            status: "error".into(),
            detail: String::new(),
            model_latencies: Vec::new(),
        };

        if !account.enabled {
            result.status = "disabled".into();
            result.detail = "账号已禁用".into();
            return result;
        }

        match self.client.test_model(plaintext_key, test_model).await {
            Ok(lat) => {
                result.is_healthy = true;
                result.status = "healthy".into();
                result.detail = format!("测试正常，延迟 {}ms", lat);
                result.model_latencies.push((test_model.to_string(), lat));
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("429") || err_str.contains("速率限制") {
                    result.status = "rate_limited".into();
                } else if err_str.contains("401") || err_str.contains("403") {
                    result.status = "unauthorized".into();
                } else {
                    result.status = "error".into();
                }
                result.detail = err_str;
            }
        }

        result
    }

    pub async fn test_candidates(
        &self,
        plaintext_key: &str,
        candidates: &[NvidiaModelItem],
    ) -> Vec<(String, bool, Option<u64>, Option<String>)> {
        let sem = Arc::new(Semaphore::new(self.settings.preflight_concurrency.max(1)));
        let mut handles = Vec::new();

        for candidate in candidates {
            let permit = sem.clone().acquire_owned().await.unwrap();
            let client = self.client.clone();
            let key = plaintext_key.to_string();
            let model_id = candidate.id.clone();

            handles.push(tokio::spawn(async move {
                let _permit = permit;
                match client.test_model(&key, &model_id).await {
                    Ok(lat) => (model_id, true, Some(lat), None),
                    Err(e) => (model_id, false, None, Some(e.to_string())),
                }
            }));
        }

        let mut results = Vec::new();
        for h in handles {
            if let Ok(res) = h.await {
                results.push(res);
            }
        }
        results
    }
}
