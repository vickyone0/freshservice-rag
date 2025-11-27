use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub name: String,
    pub description: String,
    pub method: String,
    pub path: String,
    pub parameters: Vec<ApiParameter>,
    pub curl_example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapedDocumentation {
    pub base_url: String,
    pub endpoints: Vec<ApiEndpoint>,
    pub scraped_at: chrono::DateTime<chrono::Utc>,
}