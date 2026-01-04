use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    pub id: Uuid,
    pub kind: String,
    pub url: String,
    pub auth_token: Option<String>,
    pub schedule_cron: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedDocument {
    pub id: Uuid,
    pub source_id: Uuid,
    pub title: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

pub async fn fetch_github_readme(url: &str) -> anyhow::Result<(String, String)> {
    let resp = reqwest::get(url).await?.text().await?;
    Ok(("README".to_string(), resp))
}
