use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `session` domain accessor.
#[derive(Debug)]
pub struct Session<'a> {
    session: &'a BiDiSession,
}

impl<'a> Session<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Subscribe to events for the given event methods and browsing contexts.
    /// Pass `events = &[]` and `contexts = &[]` to subscribe globally.
    pub async fn subscribe(&self, events: &[&str], contexts: &[&str]) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "events": events,
            "contexts": contexts,
        });
        self.session.send_command("session.subscribe", params).await?;
        Ok(())
    }

    /// Unsubscribe from events.
    pub async fn unsubscribe(&self, events: &[&str], contexts: &[&str]) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "events": events,
            "contexts": contexts,
        });
        self.session.send_command("session.unsubscribe", params).await?;
        Ok(())
    }
}
