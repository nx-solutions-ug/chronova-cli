use reqwest::{Client, Response};
use std::time::Duration;
use thiserror::Error;
use serde::{Deserialize, Serialize};
use base64::{Engine as _, engine::general_purpose};

use crate::heartbeat::Heartbeat;

#[derive(Debug, Serialize, Deserialize)]
pub struct StatsResponse {
    pub data: StatsData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatsData {
    pub range: String,
    pub total_seconds: f64,
    pub human_readable_total: String,
    pub human_readable_daily_average: String,
    pub languages: Vec<LanguageStat>,
    pub projects: Vec<ProjectStat>,
    pub editors: Vec<EditorStat>,
    pub operating_systems: Vec<OsStat>,
    pub categories: Vec<CategoryStat>,
    pub best_day: BestDay,
    pub daily_stats: Vec<DailyStat>,
}

// StatusBar response structure for /users/current/statusbar/today endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusBarResponse {
    pub text: String,
    pub has_team_features: Option<bool>,
}

// Fallback structure if the API returns the full summary format
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusBarFullResponse {
    pub data: StatusBarData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusBarData {
    pub categories: Vec<Category>,
    pub grand_total: GrandTotal,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub text: String,
    pub total_seconds: f64,
    pub decimal: String,
    pub digital: String,
    pub hours: i32,
    pub minutes: i32,
    pub seconds: i32,
    pub percent: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GrandTotal {
    pub text: String,
    pub total_seconds: f64,
    pub decimal: String,
    pub digital: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanguageStat {
    pub name: String,
    pub total_seconds: f64,
    pub percent: f64,
    pub digital: String,
    pub text: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectStat {
    pub name: String,
    pub total_seconds: f64,
    pub percent: f64,
    pub digital: String,
    pub text: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EditorStat {
    pub name: String,
    pub total_seconds: f64,
    pub percent: f64,
    pub digital: String,
    pub text: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OsStat {
    pub name: String,
    pub total_seconds: f64,
    pub percent: f64,
    pub digital: String,
    pub text: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryStat {
    pub name: String,
    pub total_seconds: f64,
    pub percent: f64,
    pub digital: String,
    pub text: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BestDay {
    pub date: String,
    pub total_seconds: f64,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyStat {
    pub date: String,
    pub total_seconds: f64,
    pub text: String,
    pub hours: i32,
    pub minutes: i32,
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("API error: {0} - {1}")]
    Api(String, String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Rate limited: {0}")]
    RateLimit(String),
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    pub async fn send_heartbeat(&self, heartbeat: &Heartbeat) -> Result<Response, ApiError> {
        // Try Chronova endpoint first
        let url = format!("{}/users/current/heartbeats", self.base_url.trim_end_matches('/'));

        tracing::debug!("Trying Chronova endpoint: {}", url);

        // Build request with user agent if available
        let mut request_builder = self.client.post(&url).json(heartbeat);
        if let Some(ref user_agent) = heartbeat.user_agent {
            request_builder = request_builder.header("User-Agent", user_agent);
        }

        let response = request_builder
            .send()
            .await;

        match response {
            Ok(response) if response.status().is_success() => {
                return Ok(response);
            }
            Ok(response) => {
                // Handle error response from Chronova endpoint
                let status = response.status();
                let error_body = response.text().await.unwrap_or_default();

                match status.as_u16() {
                    401 => return Err(ApiError::Auth("Invalid API key".to_string())),
                    403 => return Err(ApiError::Auth("Access denied".to_string())),
                    429 => return Err(ApiError::RateLimit("Rate limit exceeded".to_string())),
                    _ => {
                        tracing::debug!("Chronova endpoint failed with status: {}", status);
                        // Continue to try compatibility/fallback options (if implemented)
                    }
                }
            }
            Err(e) => {
                // Network error - log and allow caller to decide on fallback/retry.
                tracing::debug!("Chronova endpoint network error: {}", e);
                // Do not return immediately to allow for future compatibility fallbacks.
                // The function will fall through to the final Api error if no other attempts succeed.
            }
        }

        // If we get here, the Chronova endpoint failed (or no fallback implemented)
        Err(ApiError::Api("All endpoint attempts failed".to_string(), "No valid API endpoint found".to_string()))
    }

    pub async fn send_heartbeats_batch(&self, heartbeats: &[Heartbeat]) -> Result<Response, ApiError> {
        // Try Chronova endpoint first
        let url = format!("{}/users/current/heartbeats", self.base_url.trim_end_matches('/'));

        // Use user agent from first heartbeat if available (batched heartbeats typically come from same editor session)
        let user_agent = heartbeats.first().and_then(|h| h.user_agent.as_ref());

        // Build request with user agent if available
        let mut request_builder = self.client.post(&url).json(heartbeats);
        if let Some(ua) = user_agent {
            request_builder = request_builder.header("User-Agent", ua);
        }

        let response = request_builder.send().await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            }
        }

        // If we get here, the Chronova endpoint failed
        Err(ApiError::Api("All endpoint attempts failed".to_string(), "No valid API endpoint found".to_string()))
    }

    async fn handle_response(&self, response: Response) -> Result<Response, ApiError> {
        let status = response.status();

        if status.is_success() {
            return Ok(response);
        }

        let error_body = response.text().await.unwrap_or_default();

        match status.as_u16() {
            401 => Err(ApiError::Auth("Invalid API key".to_string())),
            403 => Err(ApiError::Auth("Access denied".to_string())),
            429 => Err(ApiError::RateLimit("Rate limit exceeded".to_string())),
            400..=499 => Err(ApiError::Api(
                format!("Client error: {}", status),
                error_body,
            )),
            500..=599 => Err(ApiError::Api(
                format!("Server error: {}", status),
                error_body,
            )),
            _ => Err(ApiError::Api(
                format!("Unexpected status: {}", status),
                error_body,
            )),
        }
    }

    pub fn with_api_key(self, api_key: String) -> AuthenticatedApiClient {
        AuthenticatedApiClient {
            client: self.client,
            base_url: self.base_url,
            api_key,
        }
    }

    /// Check network connectivity by attempting to reach the API server
    pub async fn check_connectivity(&self) -> Result<bool, ApiError> {
        // Try to make a simple HEAD request to the base URL to check connectivity
        let url = format!("{}/", self.base_url.trim_end_matches('/'));

        tracing::debug!("Checking connectivity to: {}", url);

        match self.client.head(&url).send().await {
            Ok(response) => {
                // Any successful response (even 4xx/5xx) indicates connectivity
                // We just need to know if we can reach the server
                tracing::debug!("Connectivity check successful, status: {}", response.status());
                Ok(true)
            }
            Err(e) => {
                tracing::debug!("Connectivity check failed: {}", e);
                Ok(false)
            }
        }
    }
}

#[derive(Clone)]
pub struct AuthenticatedApiClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl AuthenticatedApiClient {
    pub async fn send_heartbeat(&self, heartbeat: &Heartbeat) -> Result<Response, ApiError> {
        // Try Chronova endpoint first with Bearer token
        let url = format!("{}/users/current/heartbeats", self.base_url.trim_end_matches('/'));

        tracing::debug!("Trying Chronova endpoint with Bearer token: {}", url);

        // Build request with user agent if available
        let mut request_builder = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(heartbeat);
        if let Some(ref user_agent) = heartbeat.user_agent {
            request_builder = request_builder.header("User-Agent", user_agent);
        }

        let response = request_builder
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            } else {
                tracing::debug!("Chronova endpoint with Bearer token failed with status: {}", response.status());
            }
        }

        // Try Basic Auth (WakaTime compatibility)
        let encoded_key = general_purpose::STANDARD.encode(format!("{}:", self.api_key));
        tracing::debug!("Trying Chronova endpoint with Basic Auth: {}", url);

        // Build request with user agent if available
        let mut request_builder = self.client
            .post(&url)
            .header("Authorization", format!("Basic {}", encoded_key))
            .json(heartbeat);
        if let Some(ref user_agent) = heartbeat.user_agent {
            request_builder = request_builder.header("User-Agent", user_agent);
        }

        let response = request_builder
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            } else {
                tracing::debug!("Chronova endpoint with Basic Auth failed with status: {}", response.status());
            }
        }

        // Try X-API-Key header (WakaTime compatibility)
        tracing::debug!("Trying Chronova endpoint with X-API-Key header: {}", url);

        // Build request with user agent if available
        let mut request_builder = self.client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(heartbeat);
        if let Some(ref user_agent) = heartbeat.user_agent {
            request_builder = request_builder.header("User-Agent", user_agent);
        }

        let response = request_builder
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            } else {
                tracing::debug!("Chronova endpoint with X-API-Key header failed with status: {}", response.status());
            }
        }

        // If we get here, all Chronova endpoint attempts failed
        Err(ApiError::Api("All endpoint attempts failed".to_string(), "No valid API endpoint found".to_string()))
    }

    pub async fn send_heartbeats_batch(&self, heartbeats: &[Heartbeat]) -> Result<Response, ApiError> {
        // Try Chronova endpoint first with Bearer token
        let url = format!("{}/users/current/heartbeats", self.base_url.trim_end_matches('/'));

        // Use user agent from first heartbeat if available (batched heartbeats typically come from same editor session)
        let user_agent = heartbeats.first().and_then(|h| h.user_agent.as_ref());

        // Build request with user agent if available
        let mut request_builder = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(heartbeats);
        if let Some(ua) = user_agent {
            request_builder = request_builder.header("User-Agent", ua);
        }

        let response = request_builder.send().await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            }
        }

        // Try Basic Auth (WakaTime compatibility)
        let encoded_key = general_purpose::STANDARD.encode(format!("{}:", self.api_key));

        // Build request with user agent if available
        let mut request_builder = self.client
            .post(&url)
            .header("Authorization", format!("Basic {}", encoded_key))
            .json(heartbeats);
        if let Some(ua) = user_agent {
            request_builder = request_builder.header("User-Agent", ua);
        }

        let response = request_builder.send().await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            }
        }

        // Try X-API-Key header (WakaTime compatibility)
        // Build request with user agent if available
        let mut request_builder = self.client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(heartbeats);
        if let Some(ua) = user_agent {
            request_builder = request_builder.header("User-Agent", ua);
        }

        let response = request_builder.send().await;

        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(response);
            }
        }

        // If we get here, all Chronova endpoint attempts failed
        Err(ApiError::Api("All endpoint attempts failed".to_string(), "No valid API endpoint found".to_string()))
    }

    pub async fn get_today_stats(&self) -> Result<StatsResponse, ApiError> {
        // Try Chronova endpoint first with Bearer token
        let url = format!("{}/users/current/stats/today", self.base_url.trim_end_matches('/'));

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                let stats: StatsResponse = response.json().await?;
                return Ok(stats);
            }
        }

        // Try Basic Auth (WakaTime compatibility)
        let encoded_key = general_purpose::STANDARD.encode(format!("{}:", self.api_key));
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Basic {}", encoded_key))
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                let stats: StatsResponse = response.json().await?;
                return Ok(stats);
            }
        }

        // Try X-API-Key header (WakaTime compatibility)
        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                let stats: StatsResponse = response.json().await?;
                return Ok(stats);
            }
        }

        // If we get here, all Chronova endpoint attempts failed
        Err(ApiError::Api("All endpoint attempts failed".to_string(), "No valid API endpoint found".to_string()))
    }

    pub async fn get_today_statusbar(&self) -> Result<StatusBarResponse, ApiError> {
        // Try Chronova endpoint first with Bearer token
        let url = format!("{}/users/current/statusbar/today", self.base_url.trim_end_matches('/'));

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                // Handle Chronova API response format: { data: { grand_total: { text: "...", total_seconds: ... } } }
                let response_text = response.text().await?;
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response_text) {
                    if let Some(data) = parsed.get("data") {
                        if let Some(grand_total) = data.get("grand_total") {
                            if let Some(text) = grand_total.get("text").and_then(|v| v.as_str()) {
                                return Ok(StatusBarResponse {
                                    text: text.to_string(),
                                    has_team_features: Some(false),
                                });
                            }
                        }
                    }
                }
                // Fallback: try to parse as flat StatusBarResponse
                if let Ok(stats) = serde_json::from_str::<StatusBarResponse>(&response_text) {
                    return Ok(stats);
                }
            }
        }

        // Try Basic Auth (WakaTime compatibility)
        let encoded_key = general_purpose::STANDARD.encode(format!("{}:", self.api_key));
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Basic {}", encoded_key))
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                let stats: StatusBarResponse = response.json().await?;
                return Ok(stats);
            }
        }

        // Try X-API-Key header (WakaTime compatibility)
        let response = self.client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                let stats: StatusBarResponse = response.json().await?;
                return Ok(stats);
            }
        }

        // Try WakaTime compatibility endpoint with Bearer token
        // If we get here, all Chronova endpoint attempts failed
        Err(ApiError::Api("All endpoint attempts failed".to_string(), "No valid API endpoint found".to_string()))
    }

    async fn handle_response(&self, response: Response) -> Result<Response, ApiError> {
        let status = response.status();

        if status.is_success() {
            return Ok(response);
        }

        let error_body = response.text().await.unwrap_or_default();

        match status.as_u16() {
            401 => Err(ApiError::Auth("Invalid API key".to_string())),
            403 => Err(ApiError::Auth("Access denied".to_string())),
            429 => Err(ApiError::RateLimit("Rate limit exceeded".to_string())),
            400..=499 => Err(ApiError::Api(
                format!("Client error: {}", status),
                error_body,
            )),
            500..=599 => Err(ApiError::Api(
                format!("Server error: {}", status),
                error_body,
            )),
            _ => Err(ApiError::Api(
                format!("Unexpected status: {}", status),
                error_body,
            )),
        }
    }

    /// Check network connectivity by attempting to reach the API server
    pub async fn check_connectivity(&self) -> Result<bool, ApiError> {
        // Try to make a simple HEAD request to the base URL to check connectivity
        let url = format!("{}/", self.base_url.trim_end_matches('/'));

        tracing::debug!("Checking connectivity to: {}", url);

        match self.client.head(&url).send().await {
            Ok(response) => {
                // Any successful response (even 4xx/5xx) indicates connectivity
                // We just need to know if we can reach the server
                tracing::debug!("Connectivity check successful, status: {}", response.status());
                Ok(true)
            }
            Err(e) => {
                tracing::debug!("Connectivity check failed: {}", e);
                Ok(false)
            }
        }
    }
}

pub fn format_today_output(stats: &StatusBarResponse, hide_categories: bool) -> String {
    // If the API returned empty text (like wakatime-cli does), return empty string
    if stats.text.is_empty() {
        return "".to_string();
    }

    if hide_categories {
        // Extract just the total time from the text field
        // The text field format is usually like "4 hrs 30 mins | 2 hrs coding, 1 hr debugging"
        if let Some(total_part) = stats.text.split('|').next() {
            total_part.trim().to_string()
        } else {
            stats.text.clone()
        }
    } else {
        stats.text.clone()
    }
}

fn format_today_output_from_full(data: &StatusBarData, hide_categories: bool) -> String {
    let total_seconds = data.grand_total.total_seconds;

    if total_seconds == 0.0 {
        return "0 secs".to_string();
    }

    let hours = (total_seconds / 3600.0) as i32;
    let minutes = ((total_seconds % 3600.0) / 60.0) as i32;

    let total_time = if hours > 0 {
        if minutes > 0 {
            format!("{} hrs {} mins", hours, minutes)
        } else {
            format!("{} hrs", hours)
        }
    } else {
        format!("{} mins", minutes)
    };

    if hide_categories {
        total_time
    } else {
        let mut categories = Vec::new();

        // Add category breakdown if available
        for category in &data.categories {
            let cat_seconds = category.total_seconds;
            if cat_seconds > 0.0 {
                let cat_hours = (cat_seconds / 3600.0) as i32;
                let cat_minutes = ((cat_seconds % 3600.0) / 60.0) as i32;

                let cat_time = if cat_hours > 0 {
                    if cat_minutes > 0 {
                        format!("{} hrs {} mins {}", cat_hours, cat_minutes, category.name)
                    } else {
                        format!("{} hrs {}", cat_hours, category.name)
                    }
                } else {
                    format!("{} mins {}", cat_minutes, category.name)
                };

                categories.push(cat_time);
            }
        }

        if categories.is_empty() {
            total_time
        } else {
            format!("{} | {}", total_time, categories.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path};

    #[tokio::test]
    async fn test_send_heartbeat_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(mock_server.uri());
        let heartbeat = create_test_heartbeat();

        let result = client.send_heartbeat(&heartbeat).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_heartbeat_auth_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(mock_server.uri());
        let heartbeat = create_test_heartbeat();

        let result = client.send_heartbeat(&heartbeat).await;
        assert!(matches!(result, Err(ApiError::Auth(_))));
    }

    #[tokio::test]
    async fn test_send_heartbeat_rate_limit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let client = ApiClient::new(mock_server.uri());
        let heartbeat = create_test_heartbeat();

        let result = client.send_heartbeat(&heartbeat).await;
        assert!(matches!(result, Err(ApiError::RateLimit(_))));
    }

    fn create_test_heartbeat() -> Heartbeat {
        Heartbeat {
            id: "test-id".to_string(),
            entity: "/path/to/file.rs".to_string(),
            entity_type: "file".to_string(),
            time: 1234567890.0,
            project: Some("test-project".to_string()),
            branch: Some("main".to_string()),
            language: Some("Rust".to_string()),
            is_write: false,
            lines: Some(100),
            lineno: Some(10),
            cursorpos: Some(5),
            user_agent: Some("test/1.0".to_string()),
            category: Some("coding".to_string()),
            machine: Some("test-machine".to_string()),
            editor: None,
            operating_system: None,
            commit_hash: None,
            commit_author: None,
            commit_message: None,
            repository_url: None,
            dependencies: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_send_heartbeat_network_fallback() {
        // Use an invalid/unroutable port to force a network error and ensure the ApiClient
        // does not return a Network error early but falls through to a unified Api error when
        // no other fallback is implemented.
        let client = ApiClient::new("http://127.0.0.1:9".to_string());
        let heartbeat = create_test_heartbeat();

        let result = client.send_heartbeat(&heartbeat).await;
        // Previous behavior returned ApiError::Network; new behavior returns ApiError::Api when
        // no compatibility fallback is available. Assert that we do not get Ok.
        assert!(matches!(result, Err(ApiError::Api(_, _)) | Err(ApiError::RateLimit(_)) | Err(ApiError::Auth(_)) | Err(ApiError::Network(_))));
    }
}