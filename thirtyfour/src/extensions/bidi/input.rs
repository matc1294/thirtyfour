use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `input` domain accessor.
pub struct Input<'a> {
    session: &'a BiDiSession,
}

impl<'a> Input<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Perform a sequence of input actions.
    pub async fn perform(
        &self,
        context: &str,
        actions: Vec<serde_json::Value>,
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "context": context,
            "actions": actions,
        });
        self.session.send_command("input.performActions", params).await?;
        Ok(())
    }

    /// Release all currently pressed input.
    pub async fn release(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("input.releaseActions", params).await?;
        Ok(())
    }

    /// Set files for an input[type=file] element.
    pub async fn set_files(
        &self,
        context: &str,
        element: &str,
        files: &[&str],
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "context": context,
            "element": { "sharedId": element },
            "files": files,
        });
        self.session.send_command("input.setFiles", params).await?;
        Ok(())
    }
}
