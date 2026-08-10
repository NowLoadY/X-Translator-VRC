use std::time::Duration;

use reqwest::Method;
use serde_json::Value;

use crate::TransportError;

/// A JSON HTTP request made by an OpenAI-compatible adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl HttpRequest {
    pub fn post_json(url: impl Into<String>, body: Value) -> Self {
        Self {
            method: "POST".into(),
            url: url.into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body,
        }
    }
}

/// The owned response returned by an [`AsyncHttpClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Minimal async transport seam for production HTTP and deterministic tests.
///
/// Test implementations normally record [`HttpRequest`] and return a fixture;
/// they never need a socket or a model process.
pub trait AsyncHttpClient: Send + Sync {
    fn execute(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send;
}

/// `reqwest` implementation used by the native backend.
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    /// Builds a client with the request timeout used by the legacy backend.
    pub fn new(timeout: Duration) -> Result<Self, TransportError> {
        reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map(|client| Self { client })
            .map_err(|error| TransportError::new("client", error.to_string()))
    }

    /// Builds a client that bypasses environment-configured proxies.
    ///
    /// Local llama.cpp endpoints must never be routed through an enterprise
    /// HTTP proxy: proxies commonly reject loopback URLs with a 502, and any
    /// detour would add latency to the real-time audio path. Remote providers
    /// should continue to use [`Self::new`] so their proxy configuration is
    /// respected.
    pub fn new_direct(timeout: Duration) -> Result<Self, TransportError> {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(timeout)
            .build()
            .map(|client| Self { client })
            .map_err(|error| TransportError::new("client", error.to_string()))
    }

    /// The standard 30-second client for local `llama-server` requests.
    pub fn with_default_timeout() -> Result<Self, TransportError> {
        Self::new(Duration::from_secs(30))
    }

    /// The standard proxy-bypassing client for local `llama-server` requests.
    pub fn with_default_direct_timeout() -> Result<Self, TransportError> {
        Self::new_direct(Duration::from_secs(30))
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::with_default_timeout().expect("a default reqwest client must be constructible")
    }
}

impl AsyncHttpClient for ReqwestClient {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        let method = Method::from_bytes(request.method.as_bytes())
            .map_err(|error| TransportError::new("method", error.to_string()))?;
        let mut builder = self
            .client
            .request(method, &request.url)
            .json(&request.body);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| TransportError::new(request_error_kind(&error), error.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|error| TransportError::new("response_body", error.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}

fn request_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_request() {
        "request"
    } else {
        "http"
    }
}
