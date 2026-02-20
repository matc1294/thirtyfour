use super::BiDiSession;
use crate::error::WebDriverResult;
use serde::Deserialize;

/// Script domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ScriptEvent {
    /// A new script realm was created.
    #[serde(rename = "script.realmCreated")]
    RealmCreated(RealmInfo),
    /// A script realm was destroyed.
    #[serde(rename = "script.realmDestroyed")]
    RealmDestroyed(RealmDestroyedParams),
}

/// Information about a script realm.
#[derive(Debug, Clone, Deserialize)]
pub struct RealmInfo {
    /// The unique realm identifier.
    pub realm: String,
    /// The origin of the realm.
    pub origin: String,
    /// The type of realm (e.g., "window", "worker").
    #[serde(rename = "type")]
    pub kind: String,
    /// The browsing context (if available).
    pub context: Option<String>,
}

/// Parameters for the realm destroyed event.
#[derive(Debug, Clone, Deserialize)]
pub struct RealmDestroyedParams {
    /// The destroyed realm identifier.
    pub realm: String,
}

/// Result of a `script.evaluate` command.
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateResult {
    /// The type of result (e.g., "undefined", "string", "number").
    #[serde(rename = "type")]
    pub kind: String,
    /// The result value.
    pub result: Option<serde_json::Value>,
    /// Exception details if an error occurred.
    #[serde(rename = "exceptionDetails")]
    pub exception_details: Option<serde_json::Value>,
}

/// BiDi `script` domain accessor.
#[derive(Debug)]
pub struct Script<'a> {
    session: &'a BiDiSession,
}

impl<'a> Script<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Evaluate an expression in the given realm or context.
    pub async fn evaluate(
        &self,
        expression: &str,
        target: &str,
        await_promise: bool,
    ) -> WebDriverResult<EvaluateResult> {
        let params = serde_json::json!({
            "expression": expression,
            "target": { "realm": target },
            "awaitPromise": await_promise,
        });
        let result = self.session.send_command("script.evaluate", params).await?;
        serde_json::from_value(result)
            .map_err(|e| crate::error::WebDriverError::BiDi(format!("parse error: {e}")))
    }

    /// Add a preload script that runs before every page load. Returns the script id.
    pub async fn add_preload_script(&self, function_declaration: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({
            "functionDeclaration": function_declaration,
        });
        let result = self.session.send_command("script.addPreloadScript", params).await?;
        result["script"].as_str().map(String::from).ok_or_else(|| {
            crate::error::WebDriverError::BiDi(
                "missing 'script' in addPreloadScript response".to_string(),
            )
        })
    }

    /// Remove a preload script by id.
    pub async fn remove_preload_script(&self, script_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "script": script_id });
        self.session.send_command("script.removePreloadScript", params).await?;
        Ok(())
    }

    /// Call a function in the given realm or context.
    pub async fn call_function(
        &self,
        function_declaration: &str,
        target: &str,
        await_promise: bool,
        args: Vec<serde_json::Value>,
    ) -> WebDriverResult<EvaluateResult> {
        let params = serde_json::json!({
            "functionDeclaration": function_declaration,
            "target": { "realm": target },
            "awaitPromise": await_promise,
            "arguments": args,
        });
        let result = self.session.send_command("script.callFunction", params).await?;
        serde_json::from_value(result)
            .map_err(|e| crate::error::WebDriverError::BiDi(format!("parse error: {e}")))
    }

    /// Disown realm handles.
    pub async fn disown(&self, realm: &str, handles: &[&str]) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "target": { "realm": realm },
            "handles": handles,
        });
        self.session.send_command("script.disown", params).await?;
        Ok(())
    }
}
