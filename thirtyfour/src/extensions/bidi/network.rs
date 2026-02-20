use super::{BiDiEvent, BiDiSession};
use crate::error::WebDriverResult;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// The phase at which to intercept network requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterceptPhase {
    /// Intercept before the request is sent.
    BeforeRequestSent,
    /// Intercept after response headers are received.
    ResponseStarted,
    /// Intercept when authentication is required.
    AuthRequired,
}

/// A network request intercepted by BiDi.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkRequest {
    /// The HTTP method (e.g., "GET", "POST").
    pub method: String,
    /// The request URL.
    pub url: String,
    /// Request headers.
    pub headers: Option<Vec<Header>>,
    /// The request body size in bytes.
    pub body_size: Option<u64>,
}

/// A network response.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkResponse {
    /// The response URL.
    pub url: String,
    /// The HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Option<Vec<Header>>,
    /// The response body size in bytes.
    pub body_size: Option<u64>,
}

/// An HTTP header.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Header {
    /// The header name.
    pub name: String,
    /// The header value.
    pub value: HeaderValue,
}

/// The value of an HTTP header.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum HeaderValue {
    /// A string header value.
    String(StringValue),
    /// A base64-encoded header value.
    Base64(Base64Value),
}

/// A string header value.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StringValue {
    /// The type indicator (always "string").
    #[serde(rename = "type")]
    pub kind: String,
    /// The string value.
    pub value: String,
}

/// A base64-encoded header value.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Base64Value {
    /// The type indicator (always "base64").
    #[serde(rename = "type")]
    pub kind: String,
    /// The base64-encoded value.
    pub value: String,
}

/// Network domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum NetworkEvent {
    /// A network request is about to be sent.
    #[serde(rename = "network.beforeRequestSent")]
    BeforeRequestSent(BeforeRequestSentParams),
    /// Response headers have been received.
    #[serde(rename = "network.responseStarted")]
    ResponseStarted(ResponseStartedParams),
    /// The full response has been received.
    #[serde(rename = "network.responseCompleted")]
    ResponseCompleted(ResponseCompletedParams),
    /// A network request failed.
    #[serde(rename = "network.fetchError")]
    FetchError(FetchErrorParams),
    /// Authentication is required for the request.
    #[serde(rename = "network.authRequired")]
    AuthRequired(AuthRequiredParams),
}

/// Parameters for the `network.beforeRequestSent` event.
#[derive(Debug, Clone, Deserialize)]
pub struct BeforeRequestSentParams {
    /// The unique request identifier.
    pub request_id: String,
    /// The network request details.
    pub request: NetworkRequest,
    /// The browsing context (if available).
    pub context: Option<String>,
    /// Active intercepts for this request.
    pub intercepts: Option<Vec<String>>,
}

/// Parameters for the `network.responseStarted` event.
#[derive(Debug, Clone, Deserialize)]
pub struct ResponseStartedParams {
    /// The unique request identifier.
    pub request_id: String,
    /// The network request details.
    pub request: NetworkRequest,
    /// The network response details.
    pub response: NetworkResponse,
    /// The browsing context (if available).
    pub context: Option<String>,
}

/// Parameters for the `network.responseCompleted` event.
#[derive(Debug, Clone, Deserialize)]
pub struct ResponseCompletedParams {
    /// The unique request identifier.
    pub request_id: String,
    /// The network request details.
    pub request: NetworkRequest,
    /// The network response details.
    pub response: NetworkResponse,
    /// The browsing context (if available).
    pub context: Option<String>,
}

/// Parameters for the `network.fetchError` event.
#[derive(Debug, Clone, Deserialize)]
pub struct FetchErrorParams {
    /// The unique request identifier.
    pub request_id: String,
    /// The error message.
    pub error_text: String,
    /// The browsing context (if available).
    pub context: Option<String>,
}

/// Parameters for the `network.authRequired` event.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthRequiredParams {
    /// The unique request identifier.
    pub request_id: String,
    /// The network request details.
    pub request: NetworkRequest,
    /// The browsing context (if available).
    pub context: Option<String>,
}

/// BiDi `network` domain accessor.
pub struct Network<'a> {
    session: &'a BiDiSession,
}

impl<'a> Network<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Add a network intercept for the given phases. Returns the intercept id.
    pub async fn add_intercept(&self, phases: &[InterceptPhase]) -> WebDriverResult<String> {
        let params = serde_json::json!({ "phases": phases });
        let result = self.session.send_command("network.addIntercept", params).await?;
        let intercept_id = result["intercept"]
            .as_str()
            .ok_or_else(|| {
                crate::error::WebDriverError::BiDi(
                    "missing 'intercept' in addIntercept response".to_string(),
                )
            })?
            .to_string();
        Ok(intercept_id)
    }

    /// Remove a previously added intercept.
    pub async fn remove_intercept(&self, intercept_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "intercept": intercept_id });
        self.session.send_command("network.removeIntercept", params).await?;
        Ok(())
    }

    /// Continue a blocked request without modification.
    pub async fn continue_request(&self, request_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "request": request_id });
        self.session.send_command("network.continueRequest", params).await?;
        Ok(())
    }

    /// Continue a blocked response without modification.
    pub async fn continue_response(&self, request_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "request": request_id });
        self.session.send_command("network.continueResponse", params).await?;
        Ok(())
    }

    /// Fail a blocked request.
    pub async fn fail_request(&self, request_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "request": request_id });
        self.session.send_command("network.failRequest", params).await?;
        Ok(())
    }

    /// Provide a mock response for a blocked request.
    pub async fn provide_response(
        &self,
        request_id: &str,
        status_code: u16,
        body: Option<&str>,
    ) -> WebDriverResult<()> {
        let mut params = serde_json::json!({
            "request": request_id,
            "statusCode": status_code,
        });
        if let Some(b) = body {
            params["body"] = serde_json::json!({ "type": "string", "value": b });
        }
        self.session.send_command("network.provideResponse", params).await?;
        Ok(())
    }

    /// Subscribe to network events. Returns a broadcast receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<BiDiEvent> {
        self.session.subscribe_events()
    }
}
