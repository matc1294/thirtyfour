use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi CDP passthrough domain accessor.
#[derive(Debug)]
pub struct Cdp<'a> {
    session: &'a BiDiSession,
}

impl<'a> Cdp<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Send a raw CDP command through the BiDi CDP passthrough.
    pub async fn send_command(
        &self,
        method: &str,
        params: serde_json::Value,
        context: Option<&str>,
    ) -> WebDriverResult<serde_json::Value> {
        let mut payload = serde_json::json!({
            "method": method,
            "params": params,
        });
        if let Some(ctx) = context {
            payload["context"] = ctx.into();
        }
        self.session.send_command("cdp.sendCommand", payload).await
    }

    /// Resolve a CDP realm from a BiDi realm.
    pub async fn resolve_realm(&self, realm: &str) -> WebDriverResult<serde_json::Value> {
        let params = serde_json::json!({ "realm": realm });
        self.session.send_command("cdp.resolveRealm", params).await
    }
}
