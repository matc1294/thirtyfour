use super::BiDiSession;
use crate::error::WebDriverResult;
use serde::Deserialize;

/// Information about a browser user context.
#[derive(Debug, Clone, Deserialize)]
pub struct UserContextInfo {
    /// The unique user context identifier.
    #[serde(rename = "userContext")]
    pub user_context: String,
}

/// BiDi `browser` domain accessor.
#[derive(Debug)]
pub struct Browser<'a> {
    session: &'a BiDiSession,
}

impl<'a> Browser<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Close the browser.
    pub async fn close(&self) -> WebDriverResult<()> {
        self.session.send_command("browser.close", serde_json::json!({})).await?;
        Ok(())
    }

    /// Create a new user context (browser profile). Returns the user context id.
    pub async fn create_user_context(&self) -> WebDriverResult<String> {
        let result =
            self.session.send_command("browser.createUserContext", serde_json::json!({})).await?;
        result["userContext"].as_str().map(String::from).ok_or_else(|| {
            crate::error::WebDriverError::BiDi(
                "missing 'userContext' in createUserContext response".to_string(),
            )
        })
    }

    /// Remove (delete) a user context.
    pub async fn remove_user_context(&self, user_context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "userContext": user_context });
        self.session.send_command("browser.removeUserContext", params).await?;
        Ok(())
    }

    /// Get all user contexts.
    pub async fn get_user_contexts(&self) -> WebDriverResult<Vec<UserContextInfo>> {
        let result =
            self.session.send_command("browser.getUserContexts", serde_json::json!({})).await?;
        let infos: Vec<UserContextInfo> = serde_json::from_value(result["userContexts"].clone())
            .map_err(|e| crate::error::WebDriverError::BiDi(format!("parse error: {e}")))?;
        Ok(infos)
    }
}
