use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client,
};
use serde_json::{json, Value};

use crate::{
    error::AppError,
    models::{AppSettings, NvidiaAccount},
};

const OWNER_ID: &str = "ashan-nvidia-manager";

#[derive(Clone)]
pub struct NewApiClient {
    http: Client,
    base: String,
    token: String,
    user_id: String,
}

impl NewApiClient {
    pub fn new(http: Client, settings: &AppSettings, token: String) -> Result<Self, AppError> {
        if settings.newapi_base_url.trim().is_empty() {
            return Err(AppError::bad("New API 地址为空"));
        }
        if settings.newapi_admin_user_id.trim().is_empty() {
            return Err(AppError::bad("New API 管理员用户 ID 为空"));
        }
        Ok(Self {
            http,
            base: settings.newapi_base_url.trim_end_matches('/').to_string(),
            token,
            user_id: settings.newapi_admin_user_id.clone(),
        })
    }

    fn headers(&self) -> Result<HeaderMap, AppError> {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|e| anyhow::anyhow!(e))?,
        );
        h.insert(
            "new-api-user",
            HeaderValue::from_str(&self.user_id).map_err(|e| anyhow::anyhow!(e))?,
        );
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(h)
    }

    async fn json_request(
        &self,
        request: reqwest::RequestBuilder,
        context: &str,
    ) -> Result<Value, AppError> {
        let response = request.headers(self.headers()?).send().await?;
        let status = response.status();
        let text = response.text().await?;
        let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!(
                "New API 鉴权失败：{context}: HTTP {status}: {value}"
            )));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!(
                "New API 请求失败：{context}: HTTP {status}: {value}"
            )));
        }
        if value.get("success").and_then(|x| x.as_bool()) == Some(false) {
            let message = value
                .get("message")
                .map(|v| v.to_string())
                .unwrap_or_else(|| value.to_string());
            return Err(AppError::bad(format!("New API 返回失败：{context}: {message}")));
        }
        Ok(value)
    }

    pub async fn test_connection(&self) -> Result<(), AppError> {
        self.list_channels().await.map(|_| ())
    }

    pub async fn list_channels(&self) -> Result<Vec<Value>, AppError> {
        let mut all = Vec::new();
        for page in 1..=20 {
            let value = self
                .json_request(
                    self.http
                        .get(format!("{}/api/channel/", self.base))
                        .query(&[
                            ("p", page.to_string()),
                            ("page_size", "500".into()),
                            ("id_sort", "true".into()),
                            ("tag_mode", "false".into()),
                            ("status", "all".into()),
                        ]),
                    "读取渠道列表",
                )
                .await?;

            let rows = unwrap_rows(&value);
            let count = rows.len();
            all.extend(rows);
            if count < 500 {
                break;
            }
        }
        Ok(all)
    }

    pub async fn get_channel(&self, id: i64) -> Result<Value, AppError> {
        let value = self
            .json_request(
                self.http.get(format!("{}/api/channel/{}", self.base, id)),
                &format!("获取渠道 {id}"),
            )
            .await?;
        Ok(value.get("data").cloned().unwrap_or(value))
    }

    pub async fn create_channel(&self, name: &str, payload: &Value) -> Result<i64, AppError> {
        self.json_request(
            self.http
                .post(format!("{}/api/channel/", self.base))
                .json(&json!({
                    "mode": "single",
                    "channel": payload
                })),
            "创建渠道",
        )
        .await?;

        // New API does not return ID in AddChannel response, look up by name
        let channels = self.search_channels(name).await.unwrap_or_default();
        if let Some(ch) = channels
            .iter()
            .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
        {
            if let Some(id) = ch.get("id").and_then(|id| id.as_i64()) {
                return Ok(id);
            }
        }

        let all = self.list_channels().await.unwrap_or_default();
        if let Some(ch) = all
            .iter()
            .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
        {
            if let Some(id) = ch.get("id").and_then(|id| id.as_i64()) {
                return Ok(id);
            }
        }
        Err(AppError::bad("创建渠道成功但在列表中未找到新建渠道"))
    }

    pub async fn search_channels(&self, keyword: &str) -> Result<Vec<Value>, AppError> {
        let value = self
            .json_request(
                self.http
                    .get(format!("{}/api/channel/search", self.base))
                    .query(&[("keyword", keyword)]),
                "搜索渠道",
            )
            .await?;
        Ok(unwrap_rows(&value))
    }

    pub async fn update_channel_status(&self, id: i64, status: i32) -> Result<(), AppError> {
        let _ = self
            .json_request(
                self.http
                    .post(format!("{}/api/channel/{}/status", self.base, id))
                    .json(&json!({ "status": status })),
                "更新渠道状态",
            )
            .await;
        Ok(())
    }

    pub async fn update_channel(&self, payload: &Value) -> Result<(), AppError> {
        self.json_request(
            self.http
                .put(format!("{}/api/channel/", self.base))
                .json(payload),
            "更新渠道",
        )
        .await?;
        Ok(())
    }

    pub async fn sync_account_channel(
        &self,
        account: &NvidiaAccount,
        plaintext_key: &str,
        active_models: &[String],
        settings: &AppSettings,
        target_model: &str,
    ) -> Result<i64, AppError> {
        let mut models = vec![settings.alias_model.clone()];
        if !models.contains(&target_model.to_string()) {
            models.push(target_model.to_string());
        }
        if settings.expose_upstream_models {
            for m in active_models {
                if !models.contains(m) {
                    models.push(m.clone());
                }
            }
        }
        let models_str = models.join(",");

        // Compute group: union of managed_group and routing_groups
        let mut groups = vec![settings.managed_group.clone()];
        for g in &settings.routing_groups {
            if !groups.contains(g) {
                groups.push(g.clone());
            }
        }
        let group_str = groups.join(",");

        let channel_name = format!("{}[{}]", settings.channel_name_prefix, account.name);
        let status = if account.enabled && account.status != "error" && account.status != "unauthorized" {
            1 // enabled
        } else {
            2 // disabled
        };

        let mut mapping = std::collections::HashMap::new();
        mapping.insert(&settings.alias_model, target_model);
        let model_mapping_str = serde_json::to_string(&mapping).unwrap_or_default();

        if let Some(channel_id) = account.newapi_channel_id {
            // Check if exists
            if let Ok(mut current) = self.get_channel(channel_id).await {
                // Merge payload
                let mut payload = current.clone();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("id".into(), json!(channel_id));
                    obj.insert("name".into(), json!(channel_name));
                    obj.insert("type".into(), json!(settings.channel_type));
                    obj.insert("key".into(), json!(plaintext_key));
                    obj.insert("base_url".into(), json!(settings.nvidia_api_base.trim_end_matches("/v1")));
                    obj.insert("models".into(), json!(models_str));
                    obj.insert("model_mapping".into(), json!(model_mapping_str));
                    obj.insert("group".into(), json!(group_str));
                    obj.insert("groups".into(), json!(groups));
                    obj.insert("priority".into(), json!(account.priority));
                    obj.insert("weight".into(), json!(account.weight));
                    // New API PUT /api/channel/ strictly forbids "status" field in payload
                    obj.remove("status");
                    obj.insert("auto_ban".into(), json!(if settings.auto_ban { 1 } else { 0 }));
                    obj.insert("tag".into(), json!(settings.managed_tag));
                }
                self.update_channel(&payload).await?;
                let _ = self.update_channel_status(channel_id, status).await;
                return Ok(channel_id);
            }
        }

        // If no existing channel ID or not found, create new channel
        let create_payload = json!({
            "name": channel_name,
            "type": settings.channel_type,
            "key": plaintext_key,
            "base_url": settings.nvidia_api_base.trim_end_matches("/v1"),
            "models": models_str,
            "model_mapping": model_mapping_str,
            "group": group_str,
            "groups": groups,
            "priority": account.priority,
            "weight": account.weight,
            "auto_ban": if settings.auto_ban { 1 } else { 0 },
            "tag": settings.managed_tag,
            "remark": format!("managed_by:{}", OWNER_ID),
        });

        let id = self.create_channel(&channel_name, &create_payload).await?;
        let _ = self.update_channel_status(id, status).await;
        Ok(id)
    }
}

fn unwrap_rows(value: &Value) -> Vec<Value> {
    if let Some(arr) = value
        .get("data")
        .and_then(|v| v.get("items"))
        .and_then(|v| v.as_array())
    {
        return arr.clone();
    }
    if let Some(arr) = value.get("data").and_then(|v| v.as_array()) {
        return arr.clone();
    }
    if let Some(arr) = value
        .get("data")
        .and_then(|v| v.get("data"))
        .and_then(|v| v.as_array())
    {
        return arr.clone();
    }
    if let Some(arr) = value.as_array() {
        return arr.clone();
    }
    Vec::new()
}
