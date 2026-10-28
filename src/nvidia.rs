use std::time::Instant;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, models::NvidiaModelItem};

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<RawModelData>,
}

#[derive(Deserialize)]
struct RawModelData {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

#[derive(Clone)]
pub struct NvidiaClient {
    http: Client,
    base: String,
}

impl NvidiaClient {
    pub fn new(http: Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    pub async fn list_models(&self, api_key: &str) -> Result<Vec<String>, AppError> {
        let url = format!("{}/models", self.base);
        let resp = self.http.get(&url).bearer_auth(api_key).send().await?;
        let status = resp.status();
        let text = resp.text().await?;

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!("NVIDIA API 鉴权失败: HTTP {status}")));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!("NVIDIA models 查询失败: HTTP {status}: {text}")));
        }

        let parsed: ModelsResponse = serde_json::from_str(&text)?;
        let mut ids: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }

    pub async fn test_model(
        &self,
        api_key: &str,
        model: &str,
    ) -> Result<u64, AppError> {
        let url = format!("{}/chat/completions", self.base);
        let payload = json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 10,
            "temperature": 0
        });

        let start = Instant::now();
        let resp = self
            .http
            .post(&url)
            .timeout(std::time::Duration::from_secs(12))
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        let latency_ms = start.elapsed().as_millis() as u64;

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!("API Key 无效或未授权: HTTP {status}")));
        }
        if status.as_u16() == 429 {
            return Err(AppError::bad(format!("触发速率限制 (429 Rate Limit)")));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!("调用失败 HTTP {status}: {text}")));
        }

        let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        let choices = value.get("choices").and_then(|c| c.as_array());
        if choices.map(|c| c.is_empty()).unwrap_or(true) {
            return Err(AppError::bad(format!("响应 choices 为空")));
        }

        Ok(latency_ms)
    }

    pub fn known_model_catalog() -> Vec<NvidiaModelItem> {
        vec![
            NvidiaModelItem {
                id: "nvidia/nemotron-3-ultra-550b-a55b".into(),
                name: "Nemotron 3 Ultra (550B/55B)".into(),
                architecture: "Mamba-2 + MoE + Attention".into(),
                context_length: 1048576,
                max_output_tokens: 32768,
                category: "reasoning".into(),
                total_parameters: "550B".into(),
                active_parameters: "55B".into(),
                benchmark_swe: Some(71.9),
                benchmark_mmlu: Some(89.5),
                benchmark_terminal: Some(56.4),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "nvidia/nemotron-3.5-super-vl-120b-a12b".into(),
                name: "Nemotron 3.5 Super VL (120B/12B)".into(),
                architecture: "Transformer + C-RADIO Vision".into(),
                context_length: 1048576,
                max_output_tokens: 32768,
                category: "multimodal".into(),
                total_parameters: "120B".into(),
                active_parameters: "12B".into(),
                benchmark_swe: Some(71.1),
                benchmark_mmlu: Some(88.2),
                benchmark_terminal: Some(66.7),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "nvidia/nemotron-3.5-lightning-30b-a3b".into(),
                name: "Nemotron 3.5 Lightning (30B/3B)".into(),
                architecture: "Transformer MoE + MTP".into(),
                context_length: 1048576,
                max_output_tokens: 16384,
                category: "fast".into(),
                total_parameters: "30B".into(),
                active_parameters: "3B".into(),
                benchmark_swe: Some(51.6),
                benchmark_mmlu: Some(81.9),
                benchmark_terminal: Some(45.0),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "z-ai/glm-5.3".into(),
                name: "GLM 5.3 (753B MoE)".into(),
                architecture: "MoE + DSA Attention".into(),
                context_length: 1048576,
                max_output_tokens: 16384,
                category: "reasoning".into(),
                total_parameters: "753B".into(),
                active_parameters: "~40B".into(),
                benchmark_swe: Some(68.5),
                benchmark_mmlu: Some(87.0),
                benchmark_terminal: Some(62.0),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "deepseek-ai/deepseek-v4.1-flash".into(),
                name: "DeepSeek v4.1 Flash".into(),
                architecture: "Multi-head Latent Attention".into(),
                context_length: 1048576,
                max_output_tokens: 32768,
                category: "reasoning".into(),
                total_parameters: "552B".into(),
                active_parameters: "16B".into(),
                benchmark_swe: Some(69.2),
                benchmark_mmlu: Some(88.0),
                benchmark_terminal: Some(90.6),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "meta/llama-3.2-90b-vision-instruct".into(),
                name: "Llama 3.2 90B Vision".into(),
                architecture: "Dense Transformer + Vision".into(),
                context_length: 131072,
                max_output_tokens: 8192,
                category: "multimodal".into(),
                total_parameters: "88.6B".into(),
                active_parameters: "88.6B".into(),
                benchmark_swe: Some(58.0),
                benchmark_mmlu: Some(82.5),
                benchmark_terminal: Some(52.0),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "google/gemma-4-31b-it".into(),
                name: "Gemma 4 31B IT".into(),
                architecture: "Dense Transformer".into(),
                context_length: 262144,
                max_output_tokens: 8192,
                category: "dense".into(),
                total_parameters: "30.7B".into(),
                active_parameters: "30.7B".into(),
                benchmark_swe: Some(55.0),
                benchmark_mmlu: Some(81.0),
                benchmark_terminal: Some(50.0),
                is_online: true,
                latency_ms: None,
            },
            NvidiaModelItem {
                id: "meta/llama-3.2-11b-vision-instruct".into(),
                name: "Llama 3.2 11B Vision".into(),
                architecture: "Dense Transformer".into(),
                context_length: 131072,
                max_output_tokens: 8192,
                category: "fast".into(),
                total_parameters: "10.7B".into(),
                active_parameters: "10.7B".into(),
                benchmark_swe: Some(42.0),
                benchmark_mmlu: Some(73.0),
                benchmark_terminal: Some(38.0),
                is_online: true,
                latency_ms: None,
            },
        ]
    }
}
