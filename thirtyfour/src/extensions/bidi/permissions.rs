use super::BiDiSession;
use crate::error::WebDriverResult;

/// Permission state.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    /// Permission is granted.
    Granted,
    /// Permission is denied.
    Denied,
    /// Browser should prompt for permission.
    Prompt,
}

/// BiDi `permissions` domain accessor.
#[derive(Debug)]
pub struct Permissions<'a> {
    session: &'a BiDiSession,
}

impl<'a> Permissions<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Set the permission state for a given permission name and origin.
    pub async fn set_permission(
        &self,
        origin: &str,
        permission_name: &str,
        state: PermissionState,
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "origin": origin,
            "descriptor": { "name": permission_name },
            "state": state,
        });
        self.session.send_command("permissions.setPermission", params).await?;
        Ok(())
    }
}
