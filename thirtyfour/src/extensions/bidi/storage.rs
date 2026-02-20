use super::BiDiSession;
use crate::error::WebDriverResult;
use serde::{Deserialize, Serialize};

/// A BiDi cookie.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: CookieValue,
    /// Cookie domain.
    pub domain: String,
    /// Cookie path.
    pub path: Option<String>,
    /// Secure flag.
    pub secure: Option<bool>,
    /// HttpOnly flag.
    #[serde(rename = "httpOnly")]
    pub http_only: Option<bool>,
    /// SameSite policy.
    #[serde(rename = "sameSite")]
    pub same_site: Option<String>,
    /// Expiry timestamp.
    pub expiry: Option<u64>,
}

/// Cookie value (string or base64).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CookieValue {
    /// String cookie value.
    String {
        /// Value type.
        #[serde(rename = "type")]
        kind: String,
        /// Value content.
        value: String,
    },
}

/// BiDi `storage` domain accessor.
#[derive(Debug)]
pub struct Storage<'a> {
    session: &'a BiDiSession,
}

impl<'a> Storage<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Get cookies matching the given filter.
    pub async fn get_cookies(
        &self,
        filter: Option<serde_json::Value>,
    ) -> WebDriverResult<Vec<Cookie>> {
        let mut params = serde_json::json!({});
        if let Some(f) = filter {
            params["filter"] = f;
        }
        let result = self.session.send_command("storage.getCookies", params).await?;
        serde_json::from_value(
            result.get("cookies").cloned().unwrap_or(serde_json::Value::Array(vec![])),
        )
        .map_err(|e| crate::error::WebDriverError::BiDi(format!("parse error: {e}")))
    }

    /// Set a cookie in the given partition.
    pub async fn set_cookie(&self, cookie: Cookie) -> WebDriverResult<()> {
        let params = serde_json::json!({ "cookie": cookie });
        self.session.send_command("storage.setCookie", params).await?;
        Ok(())
    }

    /// Delete cookies matching the given filter.
    pub async fn delete_cookies(&self, filter: Option<serde_json::Value>) -> WebDriverResult<()> {
        let mut params = serde_json::json!({});
        if let Some(f) = filter {
            params["filter"] = f;
        }
        self.session.send_command("storage.deleteCookies", params).await?;
        Ok(())
    }
}
