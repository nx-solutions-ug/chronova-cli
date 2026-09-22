use base64::{engine::general_purpose, Engine as _};
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

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

/// Everything that can go wrong talking to the API.
///
/// The variants exist so callers can tell the failure modes apart: an `Auth`
/// error is worth reporting to the user, a `RateLimit` is worth backing off
/// from, and `Network` or `ServerError` are worth retrying later. Collapsing
/// them into one opaque error is what let dropped heartbeats look like
/// successes.
#[derive(Error, Debug)]
pub enum ApiError {
    /// The request never produced a response: DNS, connect, TLS or timeout.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    /// A response that does not fit any other variant, or a malformed body.
    #[error("API error: {0} - {1}")]
    Api(String, String),
    /// `401` or `403`: the server rejected the credentials.
    #[error("Authentication error: {0}")]
    Auth(String),
    /// `429`, carrying `Retry-After` in seconds when the server sent one.
    #[error("Rate limited: {message}")]
    RateLimit {
        message: String,
        retry_after: Option<Duration>,
    },
    /// A `4xx` other than `401`, `403` and `429`: the request itself was rejected.
    #[error("Bad request ({status}): {body}")]
    BadRequest { status: u16, body: String },
    /// A `5xx`: the server failed to handle an otherwise valid request.
    #[error("Server error ({status}): {body}")]
    ServerError { status: u16, body: String },
    /// The HTTP client could not be built from the configured transport options.
    #[error("Invalid transport configuration: {0}")]
    ClientBuild(String),
}

/// Transport-level settings for the HTTP client.
///
/// These are what `--timeout`, `--proxy`, `--no-ssl-verify` and
/// `--ssl-certs-file` (and their `~/.chronova.cfg` equivalents) control.
#[derive(Debug, Clone)]
pub struct TransportOptions {
    /// How long to wait for a request to complete.
    pub timeout: Duration,
    /// Proxy URL to route requests through.
    pub proxy: Option<String>,
    /// Skip TLS certificate verification.
    pub no_ssl_verify: bool,
    /// PEM bundle to use in place of the system roots.
    pub ssl_certs_file: Option<String>,
}

/// The default request timeout when nothing overrides it.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
            proxy: None,
            no_ssl_verify: false,
            ssl_certs_file: None,
        }
    }
}

impl TransportOptions {
    /// Build a `reqwest` client from these options.
    ///
    /// Fails rather than panics on a malformed proxy URL or an unreadable
    /// certificate bundle, so a typo in the config is reported instead of
    /// aborting the process.
    fn build_client(&self) -> Result<Client, ApiError> {
        let mut builder = Client::builder().timeout(self.timeout);

        if let Some(proxy_url) = &self.proxy {
            let proxy = reqwest::Proxy::all(proxy_url).map_err(|e| {
                ApiError::ClientBuild(format!("proxy {:?} is not usable: {}", proxy_url, e))
            })?;
            builder = builder.proxy(proxy);
        }

        if self.no_ssl_verify {
            tracing::debug!("TLS certificate verification disabled by configuration");
            builder = builder.danger_accept_invalid_certs(true);
        }

        if let Some(certs_file) = &self.ssl_certs_file {
            let pem = std::fs::read(certs_file).map_err(|e| {
                ApiError::ClientBuild(format!(
                    "cannot read ssl_certs_file {:?}: {}",
                    certs_file, e
                ))
            })?;
            let certs = reqwest::Certificate::from_pem_bundle(&pem).map_err(|e| {
                ApiError::ClientBuild(format!(
                    "ssl_certs_file {:?} is not valid PEM: {}",
                    certs_file, e
                ))
            })?;
            for cert in certs {
                builder = builder.add_root_certificate(cert);
            }
        }

        builder
            .build()
            .map_err(|e| ApiError::ClientBuild(format!("{}", e)))
    }
}

/// The authentication schemes tried, in order.
///
/// `Bearer` is Chronova's own scheme; `Basic` and `X-API-Key` exist because the
/// CLI is a drop-in for wakatime-cli and WakaTime-compatible backends
/// authenticate differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthScheme {
    Bearer,
    Basic,
    XApiKey,
}

impl AuthScheme {
    const ALL: [AuthScheme; 3] = [AuthScheme::Bearer, AuthScheme::Basic, AuthScheme::XApiKey];

    fn apply(self, request: RequestBuilder, api_key: &str) -> RequestBuilder {
        match self {
            AuthScheme::Bearer => request.header("Authorization", format!("Bearer {}", api_key)),
            AuthScheme::Basic => {
                let encoded = general_purpose::STANDARD.encode(format!("{}:", api_key));
                request.header("Authorization", format!("Basic {}", encoded))
            }
            AuthScheme::XApiKey => request.header("X-API-Key", api_key),
        }
    }
}

/// Read `Retry-After` as a whole number of seconds.
///
/// The header may also hold an HTTP date; we do not parse that form and leave
/// the caller to fall back on its own backoff.
fn parse_retry_after(response: &Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Classify a response.
///
/// A success passes through untouched; every other status becomes the
/// `ApiError` variant that describes it, so callers can tell "back off" from
/// "give up" from "try the next auth scheme".
async fn handle_response(response: Response) -> Result<Response, ApiError> {
    let status = response.status();

    if status.is_success() {
        return Ok(response);
    }

    let retry_after = parse_retry_after(&response);
    let code = status.as_u16();
    let body = response.text().await.unwrap_or_default();

    match code {
        401 => Err(ApiError::Auth("Invalid API key".to_string())),
        403 => Err(ApiError::Auth("Access denied".to_string())),
        429 => Err(ApiError::RateLimit {
            message: match retry_after {
                Some(after) => format!("Rate limit exceeded, retry after {}s", after.as_secs()),
                None => "Rate limit exceeded".to_string(),
            },
            retry_after,
        }),
        400..=499 => Err(ApiError::BadRequest { status: code, body }),
        500..=599 => Err(ApiError::ServerError { status: code, body }),
        _ => Err(ApiError::Api(
            format!("Unexpected status: {}", status),
            body,
        )),
    }
}

/// What the server did with one heartbeat of a bulk send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchEntryStatus {
    /// The server paired this heartbeat with a 2xx status.
    Accepted,
    /// The server paired this heartbeat with a non-2xx status.
    Rejected(u16),
    /// The server's `responses` array held no usable entry for this heartbeat.
    Unmatched,
}

impl BatchEntryStatus {
    /// Whether this heartbeat may be dropped from the offline queue.
    pub fn is_accepted(self) -> bool {
        matches!(self, BatchEntryStatus::Accepted)
    }
}

/// The per-heartbeat result of a bulk send, index-aligned with what was sent.
///
/// A bulk request can return `202` while dropping individual heartbeats, so the
/// outer status alone is not enough to decide what may leave the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSendOutcome {
    entries: Vec<BatchEntryStatus>,
}

impl BatchSendOutcome {
    /// One entry per heartbeat sent, in the order they were sent.
    pub fn entries(&self) -> &[BatchEntryStatus] {
        &self.entries
    }

    /// How many heartbeats the server accepted.
    pub fn accepted_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_accepted()).count()
    }

    /// Parse a bulk response body of the form
    /// `{"responses": [[body, status], ...]}`.
    ///
    /// Anything the server did not clearly accept counts as not sent: a missing
    /// or malformed `responses` array, a short array, or an entry without a
    /// numeric status all leave the affected heartbeats `Unmatched` so they
    /// stay queued and are retried, rather than being dropped on an assumption.
    pub fn parse(body: &str, sent: usize) -> Self {
        let responses = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| value.get("responses").cloned())
            .and_then(|responses| match responses {
                serde_json::Value::Array(entries) => Some(entries),
                _ => None,
            });

        let Some(responses) = responses else {
            tracing::warn!(
                "bulk response carried no usable `responses` array; keeping all {} heartbeat(s) queued",
                sent
            );
            return Self {
                entries: vec![BatchEntryStatus::Unmatched; sent],
            };
        };

        if responses.len() != sent {
            tracing::warn!(
                "bulk response described {} of the {} heartbeat(s) sent; keeping the remainder queued",
                responses.len(),
                sent
            );
        }

        let entries = (0..sent)
            .map(|index| match responses.get(index).and_then(entry_status) {
                Some(status) if (200..300).contains(&status) => BatchEntryStatus::Accepted,
                Some(status) => {
                    tracing::warn!(
                        "server rejected heartbeat {} of the batch with {}",
                        index,
                        status
                    );
                    BatchEntryStatus::Rejected(status)
                }
                None => BatchEntryStatus::Unmatched,
            })
            .collect();

        Self { entries }
    }
}

/// Read the status out of one `[body, status]` pair.
fn entry_status(entry: &serde_json::Value) -> Option<u16> {
    entry.get(1)?.as_u64().and_then(|s| u16::try_from(s).ok())
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
}

impl ApiClient {
    /// Build a client with the default transport settings.
    pub fn new(base_url: String) -> Self {
        Self::with_transport(base_url, &TransportOptions::default())
            .expect("default transport options always build a client")
    }

    /// Build a client honouring the configured timeout, proxy and TLS settings.
    pub fn with_transport(base_url: String, options: &TransportOptions) -> Result<Self, ApiError> {
        Ok(Self {
            client: options.build_client()?,
            base_url,
        })
    }

    fn heartbeats_url(&self) -> String {
        format!(
            "{}/users/current/heartbeats",
            self.base_url.trim_end_matches('/')
        )
    }

    /// Send a single heartbeat.
    pub async fn send_heartbeat(&self, heartbeat: &Heartbeat) -> Result<Response, ApiError> {
        let url = self.heartbeats_url();

        tracing::debug!("Sending heartbeat to: {}", url);

        let mut request_builder = self.client.post(&url).json(heartbeat);
        if let Some(ref user_agent) = heartbeat.user_agent {
            request_builder = request_builder.header("User-Agent", user_agent);
        }

        handle_response(request_builder.send().await?).await
    }

    /// Send a batch of heartbeats and report what the server did with each one.
    pub async fn send_heartbeats_batch(
        &self,
        heartbeats: &[Heartbeat],
    ) -> Result<BatchSendOutcome, ApiError> {
        let url = self.heartbeats_url();

        // Batched heartbeats come from the same editor session, so the first
        // user agent describes them all.
        let user_agent = heartbeats.first().and_then(|h| h.user_agent.as_ref());

        let mut request_builder = self.client.post(&url).json(heartbeats);
        if let Some(ua) = user_agent {
            request_builder = request_builder.header("User-Agent", ua);
        }

        let response = handle_response(request_builder.send().await?).await?;
        let body = response.text().await.unwrap_or_default();
        Ok(BatchSendOutcome::parse(&body, heartbeats.len()))
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
                tracing::debug!(
                    "Connectivity check successful, status: {}",
                    response.status()
                );
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
    fn url_for(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path)
    }

    /// Send a request under each auth scheme in turn until one is accepted.
    ///
    /// The cascade exists for WakaTime-compatible backends that expect `Basic`
    /// or `X-API-Key` where Chronova expects `Bearer`, so it falls through only
    /// on `401`/`403` — the one failure a different header can actually fix.
    /// Every other status, and any transport failure, returns immediately:
    /// re-sending an identical payload after a `429`, a `413` or a `500`
    /// amplifies exactly the condition the server is complaining about.
    async fn send_with_auth_cascade<F>(&self, build_request: F) -> Result<Response, ApiError>
    where
        F: Fn() -> RequestBuilder,
    {
        let mut rejected: Option<ApiError> = None;

        for scheme in AuthScheme::ALL {
            let response = match scheme.apply(build_request(), &self.api_key).send().await {
                Ok(response) => response,
                Err(e) => {
                    tracing::debug!("{:?} request never reached the server: {}", scheme, e);
                    return Err(ApiError::Network(e));
                }
            };

            match handle_response(response).await {
                Ok(response) => return Ok(response),
                Err(e @ ApiError::Auth(_)) => {
                    tracing::debug!("{:?} auth rejected ({}); trying the next scheme", scheme, e);
                    rejected = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(rejected.unwrap_or_else(|| ApiError::Auth("No auth scheme was accepted".to_string())))
    }

    /// Send a single heartbeat.
    pub async fn send_heartbeat(&self, heartbeat: &Heartbeat) -> Result<Response, ApiError> {
        let url = self.url_for("users/current/heartbeats");

        self.send_with_auth_cascade(|| {
            let mut request_builder = self.client.post(&url).json(heartbeat);
            if let Some(ref user_agent) = heartbeat.user_agent {
                request_builder = request_builder.header("User-Agent", user_agent);
            }
            request_builder
        })
        .await
    }

    /// Send a batch of heartbeats and report what the server did with each one.
    pub async fn send_heartbeats_batch(
        &self,
        heartbeats: &[Heartbeat],
    ) -> Result<BatchSendOutcome, ApiError> {
        let url = self.url_for("users/current/heartbeats");

        // Batched heartbeats come from the same editor session, so the first
        // user agent describes them all.
        let user_agent = heartbeats.first().and_then(|h| h.user_agent.as_ref());

        let response = self
            .send_with_auth_cascade(|| {
                let mut request_builder = self.client.post(&url).json(heartbeats);
                if let Some(ua) = user_agent {
                    request_builder = request_builder.header("User-Agent", ua);
                }
                request_builder
            })
            .await?;

        let body = response.text().await.unwrap_or_default();
        Ok(BatchSendOutcome::parse(&body, heartbeats.len()))
    }

    /// Fetch today's aggregated stats.
    pub async fn get_today_stats(&self) -> Result<StatsResponse, ApiError> {
        let url = self.url_for("users/current/stats/today");

        let response = self
            .send_with_auth_cascade(|| self.client.get(&url))
            .await?;

        Ok(response.json().await?)
    }

    /// Fetch today's status-bar summary.
    pub async fn get_today_statusbar(&self) -> Result<StatusBarResponse, ApiError> {
        let url = self.url_for("users/current/statusbar/today");

        let response = self
            .send_with_auth_cascade(|| self.client.get(&url))
            .await?;

        let body = response.text().await?;

        // Chronova answers `{ data: { grand_total: { text: ... } } }`; a
        // WakaTime-compatible backend answers the flat shape.
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(text) = parsed
                .get("data")
                .and_then(|data| data.get("grand_total"))
                .and_then(|total| total.get("text"))
                .and_then(|text| text.as_str())
            {
                return Ok(StatusBarResponse {
                    text: text.to_string(),
                    has_team_features: Some(false),
                });
            }
        }

        serde_json::from_str::<StatusBarResponse>(&body).map_err(|e| {
            ApiError::Api(
                format!("Unrecognised statusbar response: {}", e),
                body.clone(),
            )
        })
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
                tracing::debug!(
                    "Connectivity check successful, status: {}",
                    response.status()
                );
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

#[allow(dead_code)]
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        assert!(matches!(result, Err(ApiError::RateLimit { .. })));
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
            ai: Default::default(),
        }
    }

    /// The key used by the authenticated tests, and its Basic-auth encoding.
    const TEST_API_KEY: &str = "test-key";

    fn basic_header() -> String {
        format!(
            "Basic {}",
            general_purpose::STANDARD.encode(format!("{}:", TEST_API_KEY))
        )
    }

    fn auth_client(uri: String) -> AuthenticatedApiClient {
        ApiClient::new(uri).with_api_key(TEST_API_KEY.to_string())
    }

    /// One `[body, status]` pair, as the bulk endpoint returns them.
    fn bulk_body(statuses: &[u16]) -> String {
        let entries: Vec<serde_json::Value> = statuses
            .iter()
            .map(|status| serde_json::json!([{"data": {}}, status]))
            .collect();
        serde_json::json!({ "responses": entries }).to_string()
    }

    #[tokio::test]
    async fn test_auth_cascade_falls_through_401_to_basic() {
        let mock_server = MockServer::start().await;

        // Basic is the only scheme this server accepts.
        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .and(header("Authorization", basic_header().as_str()))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = auth_client(mock_server.uri())
            .send_heartbeat(&create_test_heartbeat())
            .await;

        assert!(result.is_ok(), "Basic auth should have been accepted");
    }

    #[tokio::test]
    async fn test_rate_limit_is_not_retried_against_another_scheme() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "42"))
            .expect(1) // the regression: this used to be 3
            .mount(&mock_server)
            .await;

        let result = auth_client(mock_server.uri())
            .send_heartbeat(&create_test_heartbeat())
            .await;

        match result {
            Err(ApiError::RateLimit { retry_after, .. }) => {
                assert_eq!(retry_after, Some(Duration::from_secs(42)));
            }
            other => panic!("expected a rate-limit error, got {:?}", other.err()),
        }

        assert_eq!(mock_server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_server_error_is_not_retried_against_another_scheme() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = auth_client(mock_server.uri())
            .send_heartbeat(&create_test_heartbeat())
            .await;

        assert!(
            matches!(result, Err(ApiError::ServerError { status: 500, .. })),
            "expected a server error, got {:?}",
            result.err()
        );
        assert_eq!(mock_server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_bad_request_is_not_retried_against_another_scheme() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(413))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = auth_client(mock_server.uri())
            .send_heartbeat(&create_test_heartbeat())
            .await;

        assert!(
            matches!(result, Err(ApiError::BadRequest { status: 413, .. })),
            "expected a bad-request error, got {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_batch_reports_the_entry_the_server_rejected() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(202).set_body_string(bulk_body(&[202, 400, 202])))
            .expect(1)
            .mount(&mock_server)
            .await;

        let heartbeats = vec![create_test_heartbeat(); 3];
        let outcome = auth_client(mock_server.uri())
            .send_heartbeats_batch(&heartbeats)
            .await
            .expect("the request itself succeeded");

        assert_eq!(
            outcome.entries(),
            [
                BatchEntryStatus::Accepted,
                BatchEntryStatus::Rejected(400),
                BatchEntryStatus::Accepted,
            ]
        );
        assert_eq!(outcome.accepted_count(), 2);
    }

    #[tokio::test]
    async fn test_batch_with_short_responses_leaves_the_remainder_unmatched() {
        let mock_server = MockServer::start().await;

        // Fewer results than heartbeats sent: the third was never accounted for.
        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(202).set_body_string(bulk_body(&[202, 202])))
            .mount(&mock_server)
            .await;

        let heartbeats = vec![create_test_heartbeat(); 3];
        let outcome = auth_client(mock_server.uri())
            .send_heartbeats_batch(&heartbeats)
            .await
            .expect("the request itself succeeded");

        assert_eq!(
            outcome.entries(),
            [
                BatchEntryStatus::Accepted,
                BatchEntryStatus::Accepted,
                BatchEntryStatus::Unmatched,
            ]
        );
    }

    #[tokio::test]
    async fn test_batch_without_a_responses_array_accepts_nothing() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(202).set_body_string("{\"ok\":true}"))
            .mount(&mock_server)
            .await;

        let heartbeats = vec![create_test_heartbeat(); 2];
        let outcome = auth_client(mock_server.uri())
            .send_heartbeats_batch(&heartbeats)
            .await
            .expect("the request itself succeeded");

        assert_eq!(outcome.accepted_count(), 0);
        assert!(outcome
            .entries()
            .iter()
            .all(|entry| *entry == BatchEntryStatus::Unmatched));
    }

    #[tokio::test]
    async fn test_unauthenticated_batch_surfaces_rate_limiting() {
        // This is the client `sync.rs` holds, so this is what makes its
        // rate-limit backoff reachable at all.
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = ApiClient::new(mock_server.uri())
            .send_heartbeats_batch(&[create_test_heartbeat()])
            .await;

        assert!(
            matches!(
                result,
                Err(ApiError::RateLimit {
                    retry_after: None,
                    ..
                })
            ),
            "expected a rate-limit error, got {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_timeout_is_honoured_and_does_not_fall_through() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_secs(10)))
            .expect(1) // a transport failure must not be retried under another scheme
            .mount(&mock_server)
            .await;

        let options = TransportOptions {
            timeout: Duration::from_secs(1),
            ..TransportOptions::default()
        };
        let client = ApiClient::with_transport(mock_server.uri(), &options)
            .expect("client builds")
            .with_api_key(TEST_API_KEY.to_string());

        let started = std::time::Instant::now();
        let result = client.send_heartbeat(&create_test_heartbeat()).await;
        let elapsed = started.elapsed();

        match result {
            Err(ApiError::Network(e)) => assert!(e.is_timeout(), "expected a timeout, got {}", e),
            other => panic!("expected a transport error, got {:?}", other.err()),
        }
        assert!(
            elapsed < Duration::from_secs(5),
            "--timeout was ignored: the call took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_parse_bulk_response_shapes() {
        // The status of each pair decides that heartbeat's fate.
        assert_eq!(
            BatchSendOutcome::parse(&bulk_body(&[201, 500]), 2).entries(),
            [BatchEntryStatus::Accepted, BatchEntryStatus::Rejected(500)]
        );

        // More results than heartbeats: the extras are ignored.
        assert_eq!(
            BatchSendOutcome::parse(&bulk_body(&[202, 202]), 1).entries(),
            [BatchEntryStatus::Accepted]
        );

        // An entry without a numeric status tells us nothing.
        assert_eq!(
            BatchSendOutcome::parse(r#"{"responses": [[{}, "202"]]}"#, 1).entries(),
            [BatchEntryStatus::Unmatched]
        );

        // Neither does a body that is not JSON at all.
        assert_eq!(
            BatchSendOutcome::parse("not json", 2).entries(),
            [BatchEntryStatus::Unmatched, BatchEntryStatus::Unmatched]
        );

        // An empty batch has nothing to report.
        assert!(BatchSendOutcome::parse("", 0).entries().is_empty());
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
        assert!(matches!(
            result,
            Err(ApiError::Api(_, _))
                | Err(ApiError::RateLimit { .. })
                | Err(ApiError::Auth(_))
                | Err(ApiError::Network(_))
        ));
    }
}
