use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `emulation` domain accessor.
pub struct Emulation<'a> {
    session: &'a BiDiSession,
}

impl<'a> Emulation<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Override the geolocation for a browsing context.
    pub async fn set_geolocation_override(
        &self,
        context: &str,
        latitude: f64,
        longitude: f64,
        accuracy: Option<f64>,
    ) -> WebDriverResult<()> {
        let mut coords = serde_json::json!({
            "latitude": latitude,
            "longitude": longitude,
        });
        if let Some(acc) = accuracy {
            coords["accuracy"] = acc.into();
        }
        let params = serde_json::json!({
            "context": context,
            "coordinates": coords,
        });
        self.session.send_command("emulation.setGeolocationOverride", params).await?;
        Ok(())
    }

    /// Set device posture for a browsing context.
    pub async fn set_device_posture(&self, context: &str, posture: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "context": context,
            "posture": posture,
        });
        self.session.send_command("emulation.setDevicePosture", params).await?;
        Ok(())
    }
}
