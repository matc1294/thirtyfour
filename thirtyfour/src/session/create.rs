use serde::Deserialize;
use url::Url;

use super::http::HttpClient;
use crate::error::WebDriverErrorInner;
use crate::{
    common::{
        command::{Command, FormatRequestData},
        config::WebDriverConfig,
    },
    prelude::WebDriverResult,
    session::http::run_webdriver_cmd,
    Capabilities, SessionId, TimeoutConfiguration,
};

#[derive(Debug, Deserialize)]
pub(crate) struct SessionCapabilities {
    #[serde(rename = "webSocketUrl")]
    web_socket_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionCreationValue {
    #[serde(default, rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    capabilities: Option<SessionCapabilities>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionCreationResponse {
    #[serde(default, rename = "sessionId")]
    session_id: String,
    value: SessionCreationValue,
}

impl SessionCreationResponse {
    fn capabilities_websocket_url(&self) -> Option<&str> {
        self.value.capabilities.as_ref().and_then(|c| c.web_socket_url.as_deref())
    }
}

/// Start a new WebDriver session, returning the session id and the
/// capabilities JSON that was received back from the server.
pub async fn start_session(
    http_client: &dyn HttpClient,
    server_url: &Url,
    config: &WebDriverConfig,
    capabilities: Capabilities,
) -> WebDriverResult<(SessionId, Option<String>)> {
    let request_data = Command::NewSession(serde_json::Value::Object(capabilities))
        .format_request(&SessionId::null());

    let v = match run_webdriver_cmd(http_client, &request_data, server_url, config).await {
        Ok(x) => Ok(x),
        Err(e) => {
            // Selenium sometimes gives a bogus 500 error "Chrome failed to start".
            // Retry if we get a 500. If it happens twice in a row, then the second error
            // will be returned.
            if let WebDriverErrorInner::UnknownError(x) = &*e {
                if x.status == 500 {
                    run_webdriver_cmd(http_client, &request_data, server_url, config).await
                } else {
                    Err(e)
                }
            } else {
                Err(e)
            }
        }
    }?;

    let resp: SessionCreationResponse = serde_json::from_value(v.body)?;
    let ws_url = resp.capabilities_websocket_url().map(String::from);
    let data = resp.value;
    let session_id = SessionId::from(if resp.session_id.is_empty() {
        data.session_id
    } else {
        resp.session_id
    });

    // Set default timeouts.
    let request_data =
        Command::SetTimeouts(TimeoutConfiguration::default()).format_request(&session_id);
    run_webdriver_cmd(http_client, &request_data, server_url, config).await?;

    Ok((session_id, ws_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_websocket_url_present() {
        let resp_json = serde_json::json!({
            "sessionId": "abc",
            "value": {
                "sessionId": "abc",
                "capabilities": {
                    "browserName": "chrome",
                    "webSocketUrl": "ws://localhost:1234/session/abc/se/bidi"
                }
            }
        });
        let resp: SessionCreationResponse = serde_json::from_value(resp_json).unwrap();
        assert_eq!(
            resp.capabilities_websocket_url(),
            Some("ws://localhost:1234/session/abc/se/bidi")
        );
    }

    #[test]
    fn test_parse_websocket_url_absent() {
        let resp_json = serde_json::json!({
            "sessionId": "abc",
            "value": {
                "sessionId": "abc",
                "capabilities": { "browserName": "firefox" }
            }
        });
        let resp: SessionCreationResponse = serde_json::from_value(resp_json).unwrap();
        assert_eq!(resp.capabilities_websocket_url(), None);
    }
}
