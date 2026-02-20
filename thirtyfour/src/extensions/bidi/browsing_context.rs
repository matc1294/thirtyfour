use super::BiDiSession;
use crate::error::WebDriverResult;
use serde::{Deserialize, Serialize};

/// How to create a new browsing context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateType {
    /// Create a new tab.
    Tab,
    /// Create a new window.
    Window,
}

/// Browsing context domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum BrowsingContextEvent {
    /// A new browsing context was created.
    #[serde(rename = "browsingContext.contextCreated")]
    ContextCreated(ContextInfo),
    /// A browsing context was destroyed.
    #[serde(rename = "browsingContext.contextDestroyed")]
    ContextDestroyed(ContextInfo),
    /// Navigation started in a browsing context.
    #[serde(rename = "browsingContext.navigationStarted")]
    NavigationStarted(NavigationInfo),
    /// DOM content loaded in a browsing context.
    #[serde(rename = "browsingContext.domContentLoaded")]
    DomContentLoaded(NavigationInfo),
    /// Page load completed in a browsing context.
    #[serde(rename = "browsingContext.load")]
    Load(NavigationInfo),
    /// Navigation was aborted in a browsing context.
    #[serde(rename = "browsingContext.navigationAborted")]
    NavigationAborted(NavigationInfo),
    /// Navigation failed in a browsing context.
    #[serde(rename = "browsingContext.navigationFailed")]
    NavigationFailed(NavigationInfo),
}

/// Information about a browsing context.
#[derive(Debug, Clone, Deserialize)]
pub struct ContextInfo {
    /// The unique browsing context identifier.
    pub context: String,
    /// The current URL of the context.
    pub url: Option<String>,
    /// The parent browsing context (if any).
    pub parent: Option<String>,
}

/// Information about a navigation event.
#[derive(Debug, Clone, Deserialize)]
pub struct NavigationInfo {
    /// The unique browsing context identifier.
    pub context: String,
    /// The navigation ID (if navigation is in progress).
    pub navigation: Option<String>,
    /// The URL being navigated to.
    pub url: String,
}

/// BiDi `browsingContext` domain accessor.
#[derive(Debug)]
pub struct BrowsingContext<'a> {
    session: &'a BiDiSession,
}

impl<'a> BrowsingContext<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Get the tree of currently open browsing contexts.
    pub async fn get_tree(&self) -> WebDriverResult<serde_json::Value> {
        self.session.send_command("browsingContext.getTree", serde_json::json!({})).await
    }

    /// Create a new browsing context (tab or window). Returns the context id.
    pub async fn create(&self, kind: CreateType) -> WebDriverResult<String> {
        let params = serde_json::json!({ "type": kind });
        let result = self.session.send_command("browsingContext.create", params).await?;
        result
            .get("context")
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| {
                crate::error::WebDriverError::BiDi(
                    "missing 'context' in browsingContext.create response".to_string(),
                )
            })
    }

    /// Close a browsing context.
    pub async fn close(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("browsingContext.close", params).await?;
        Ok(())
    }

    /// Navigate a browsing context to a URL. Returns the navigation id if available.
    pub async fn navigate(&self, context: &str, url: &str) -> WebDriverResult<Option<String>> {
        let params = serde_json::json!({ "context": context, "url": url });
        let result = self.session.send_command("browsingContext.navigate", params).await?;
        Ok(result.get("navigation").and_then(serde_json::Value::as_str).map(String::from))
    }

    /// Reload the current page in a browsing context.
    pub async fn reload(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("browsingContext.reload", params).await?;
        Ok(())
    }

    /// Activate (focus) a browsing context.
    pub async fn activate(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("browsingContext.activate", params).await?;
        Ok(())
    }

    /// Set the viewport size of a browsing context.
    pub async fn set_viewport(
        &self,
        context: &str,
        width: u32,
        height: u32,
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "context": context,
            "viewport": { "width": width, "height": height }
        });
        self.session.send_command("browsingContext.setViewport", params).await?;
        Ok(())
    }

    /// Capture a screenshot of a browsing context. Returns base64-encoded PNG bytes.
    pub async fn capture_screenshot(&self, context: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({ "context": context });
        let result = self.session.send_command("browsingContext.captureScreenshot", params).await?;
        Ok(result
            .get("data")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    /// Traverse browsing history by delta (negative = back, positive = forward).
    pub async fn traverse_history(&self, context: &str, delta: i32) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context, "delta": delta });
        self.session.send_command("browsingContext.traverseHistory", params).await?;
        Ok(())
    }
}
