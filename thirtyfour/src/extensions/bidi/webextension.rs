use std::path::Path;

use base64::{prelude::BASE64_STANDARD, Engine};

use super::BiDiSession;
use crate::error::WebDriverResult;

/// `BiDi` `webExtension` domain accessor.
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

    /// Install a web extension from an archive path on the remote end's filesystem.
    ///
    /// This uses the `archivePath` extension data type. The path should point to
    /// a `.crx`, `.xpi`, or other extension archive file on the machine running
    /// the browser.
    ///
    /// Returns the extension id.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or the response is malformed.
    pub async fn install(&self, archive_path: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({
            "extensionData": {
                "type": "archivePath",
                "path": archive_path,
            }
        });
        let result = self.session.send_command("webExtension.install", params).await?;
        result.get("extension").and_then(serde_json::Value::as_str).map(String::from).ok_or_else(
            || {
                crate::error::WebDriverError::BiDi(
                    "missing 'extension' in webExtension.install response".to_string(),
                )
            },
        )
    }

    /// Install a web extension from an unpacked directory on the remote end's filesystem.
    ///
    /// This uses the `path` extension data type. The path should point to an
    /// unpacked extension directory on the machine running the browser.
    ///
    /// Returns the extension id.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or the response is malformed.
    pub async fn install_from_directory(&self, path: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({
            "extensionData": {
                "type": "path",
                "path": path,
            }
        });
        let result = self.session.send_command("webExtension.install", params).await?;
        result.get("extension").and_then(serde_json::Value::as_str).map(String::from).ok_or_else(
            || {
                crate::error::WebDriverError::BiDi(
                    "missing 'extension' in webExtension.install response".to_string(),
                )
            },
        )
    }

    /// Install a web extension from a local file.
    ///
    /// This reads the file locally and sends it as base64-encoded data using
    /// the `base64` extension data type.
    ///
    /// Returns the extension id.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the command fails,
    /// or the response is malformed.
    pub async fn install_from_file(&self, path: &Path) -> WebDriverResult<String> {
        let contents = std::fs::read(path)?;
        let encoded = BASE64_STANDARD.encode(contents);
        self.install_encoded(&encoded).await
    }

    /// Install a web extension from base64-encoded extension data.
    ///
    /// This uses the `base64` extension data type.
    ///
    /// Returns the extension id.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or the response is malformed.
    pub async fn install_encoded(&self, extension_base64: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({
            "extensionData": {
                "type": "base64",
                "value": extension_base64,
            }
        });
        let result = self.session.send_command("webExtension.install", params).await?;
        result.get("extension").and_then(serde_json::Value::as_str).map(String::from).ok_or_else(
            || {
                crate::error::WebDriverError::BiDi(
                    "missing 'extension' in webExtension.install response".to_string(),
                )
            },
        )
    }

    /// Uninstall a web extension by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn uninstall(&self, extension_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "extension": extension_id });
        self.session.send_command("webExtension.uninstall", params).await?;
        Ok(())
    }
}
