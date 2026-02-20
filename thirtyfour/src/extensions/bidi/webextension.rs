use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `webExtension` domain accessor.
#[derive(Debug)]
pub struct WebExtension<'a> {
    session: &'a BiDiSession,
}

impl<'a> WebExtension<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Install a web extension from an archive path. Returns the extension id.
    pub async fn install(&self, archive_path: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({
            "extensionData": {
                "type": "archivePath",
                "path": archive_path,
            }
        });
        let result = self.session.send_command("webExtension.install", params).await?;
        result
            .get("extension")
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| {
                crate::error::WebDriverError::BiDi(
                    "missing 'extension' in webExtension.install response".to_string(),
                )
            })
    }

    /// Uninstall a web extension by id.
    pub async fn uninstall(&self, extension_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "extension": extension_id });
        self.session.send_command("webExtension.uninstall", params).await?;
        Ok(())
    }
}
