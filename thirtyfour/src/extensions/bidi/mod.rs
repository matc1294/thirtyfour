//! WebDriver BiDi bidirectional protocol support.
//!
//! Enable with the `bidi` cargo feature.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{broadcast, oneshot, Mutex as TokioMutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::error::{WebDriverError, WebDriverResult};

pub use browser::Browser;
pub use browsing_context::{BrowsingContext, BrowsingContextEvent};
pub use cdp::Cdp;
pub use console::Console;
pub use emulation::Emulation;
pub use input::Input;
pub use log::Log;
pub use network::{Network, NetworkEvent};
pub use permissions::Permissions;
pub use script::{Script, ScriptEvent};
pub use session::Session;
pub use storage::Storage;
pub use webextension::WebExtension;

/// Browser domain commands.
pub mod browser;
/// Browsing context domain commands and events.
pub mod browsing_context;
/// CDP passthrough domain commands.
pub mod cdp;
/// Console domain (wrapper over log).
pub mod console;
/// Emulation domain commands.
pub mod emulation;
/// Input domain commands.
pub mod input;
/// Log domain commands and events.
pub mod log;
/// Network domain commands and events.
pub mod network;
/// Permissions domain commands.
pub mod permissions;
/// Script domain commands and events.
pub mod script;
/// Session domain commands.
pub mod session;
/// Storage domain commands.
pub mod storage;
/// WebExtension domain commands.
pub mod webextension;

/// All BiDi events that can be received from the browser.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BiDiEvent {
    /// Network domain events.
    Network(NetworkEvent),
    /// Log domain events.
    Log(log::LogEvent),
    /// Script domain events.
    Script(ScriptEvent),
    /// BrowsingContext domain events.
    BrowsingContext(BrowsingContextEvent),
    /// Console domain events (alias for log.entryAdded with console source).
    Console(console::ConsoleEvent),
    /// An unrecognised event method and its raw params.
    Unknown {
        /// The event method name (e.g., "network.beforeRequestSent").
        method: String,
        /// The event parameters.
        params: Value,
    },
}

/// A live WebDriver BiDi session over a WebSocket connection.
///
/// Obtain one by calling [`WebDriver::bidi_connect`][crate::WebDriver::bidi_connect].
pub struct BiDiSession {
    /// Sends frames to the WebSocket (async mutex for safe await).
    ws_sink: Arc<
        TokioMutex<
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                Message,
            >,
        >,
    >,
    /// Auto-incrementing JSON-RPC command id.
    command_id: Arc<AtomicU64>,
    /// In-flight commands waiting for a response. Never held across `.await`.
    pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<WebDriverResult<Value>>>>>,
    /// Broadcast channel for all incoming events.
    event_tx: broadcast::Sender<BiDiEvent>,
}

impl std::fmt::Debug for BiDiSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiDiSession").finish_non_exhaustive()
    }
}

impl BiDiSession {
    /// Connect to the BiDi WebSocket endpoint and spawn the dispatch task.
    pub async fn connect(ws_url: &str) -> WebDriverResult<Self> {
        let (ws_stream, _) = connect_async(ws_url)
            .await
            .map_err(|e| WebDriverError::BiDi(format!("WebSocket connect failed: {e}")))?;

        let (sink, mut stream) = ws_stream.split();
        let (event_tx, _) = broadcast::channel::<BiDiEvent>(256);
        let command_id = Arc::new(AtomicU64::new(1));
        let pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<WebDriverResult<Value>>>>> =
            Arc::new(StdMutex::new(HashMap::new()));

        let pending_clone = Arc::clone(&pending);
        let event_tx_clone = event_tx.clone();

        // Background dispatch task.
        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t,
                    Ok(Message::Close(_)) => break,
                    Ok(_) => continue,
                    Err(e) => {
                        tracing::error!("BiDi WebSocket error: {e}");
                        break;
                    }
                };

                let v: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("BiDi: failed to parse message: {e}");
                        continue;
                    }
                };

                match v.get("type").and_then(Value::as_str) {
                    Some("success") | Some("error") => {
                        if let Some(id) = v.get("id").and_then(Value::as_u64) {
                            let sender = {
                                let mut map = pending_clone.lock().unwrap();
                                map.remove(&id)
                            };
                            if let Some(tx) = sender {
                                let result = if v["type"] == "success" {
                                    Ok(v["result"].clone())
                                } else {
                                    let msg = v["message"]
                                        .as_str()
                                        .unwrap_or("unknown BiDi error")
                                        .to_string();
                                    Err(WebDriverError::BiDi(msg))
                                };
                                let _ = tx.send(result);
                            }
                        }
                    }
                    Some("event") => {
                        let method = v["method"].as_str().unwrap_or("").to_string();
                        let params = v["params"].clone();
                        let event = parse_event(&method, params);
                        // Ignore send errors (no subscribers).
                        let _ = event_tx_clone.send(event);
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            ws_sink: Arc::new(TokioMutex::new(sink)),
            command_id,
            pending,
            event_tx,
        })
    }

    /// Send a BiDi command and await the response.
    ///
    /// `method` is e.g. `"network.addIntercept"`.
    /// `params` is the JSON params object.
    pub async fn send_command(&self, method: &str, params: Value) -> WebDriverResult<Value> {
        let id = self.command_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({ "id": id, "method": method, "params": params });
        let text = serde_json::to_string(&msg)
            .map_err(|e| WebDriverError::BiDi(format!("serialise error: {e}")))?;

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().unwrap();
            map.insert(id, tx);
        }

        // Lock the async mutex (safe across await).
        self.ws_sink
            .lock()
            .await
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| WebDriverError::BiDi(format!("WebSocket send failed: {e}")))?;

        rx.await.map_err(|_| WebDriverError::BiDi("response channel closed".to_string()))?
    }

    /// Subscribe to all BiDi events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<BiDiEvent> {
        self.event_tx.subscribe()
    }

    // --- Domain accessors ---

    /// Access the `session` domain.
    pub fn session(&self) -> Session<'_> {
        Session::new(self)
    }

    /// Access the `log` domain.
    pub fn log(&self) -> Log<'_> {
        Log::new(self)
    }

    /// Access the `network` domain.
    pub fn network(&self) -> Network<'_> {
        Network::new(self)
    }

    /// Access the `browsingContext` domain.
    pub fn browsing_context(&self) -> BrowsingContext<'_> {
        BrowsingContext::new(self)
    }

    /// Access the `script` domain.
    pub fn script(&self) -> Script<'_> {
        Script::new(self)
    }

    /// Access the `browser` domain.
    pub fn browser(&self) -> Browser<'_> {
        Browser::new(self)
    }

    /// Access the `console` domain (thin wrapper over log).
    pub fn console(&self) -> Console<'_> {
        Console::new(self)
    }

    /// Access the `input` domain.
    pub fn input(&self) -> Input<'_> {
        Input::new(self)
    }

    /// Access the `permissions` domain.
    pub fn permissions(&self) -> Permissions<'_> {
        Permissions::new(self)
    }

    /// Access the `storage` domain.
    pub fn storage(&self) -> Storage<'_> {
        Storage::new(self)
    }

    /// Access the `webExtension` domain.
    pub fn webextension(&self) -> WebExtension<'_> {
        WebExtension::new(self)
    }

    /// Access the `emulation` domain.
    pub fn emulation(&self) -> Emulation<'_> {
        Emulation::new(self)
    }

    /// Access the BiDi CDP passthrough domain.
    pub fn cdp(&self) -> Cdp<'_> {
        Cdp::new(self)
    }
}

/// Parse an incoming event message into a `BiDiEvent`.
fn parse_event(method: &str, params: Value) -> BiDiEvent {
    match method {
        "network.beforeRequestSent"
        | "network.responseStarted"
        | "network.responseCompleted"
        | "network.fetchError"
        | "network.authRequired" => {
            match serde_json::from_value::<NetworkEvent>(
                json!({ "method": method, "params": params }),
            ) {
                Ok(e) => BiDiEvent::Network(e),
                Err(_) => BiDiEvent::Unknown {
                    method: method.to_string(),
                    params,
                },
            }
        }
        "log.entryAdded" => {
            match serde_json::from_value::<log::LogEvent>(params.clone()) {
                Ok(e) => {
                    // If it's a console entry, also emit as Console variant.
                    BiDiEvent::Log(e)
                }
                Err(_) => BiDiEvent::Unknown {
                    method: method.to_string(),
                    params,
                },
            }
        }
        "script.realmCreated" | "script.realmDestroyed" => {
            match serde_json::from_value::<ScriptEvent>(
                json!({ "method": method, "params": params }),
            ) {
                Ok(e) => BiDiEvent::Script(e),
                Err(_) => BiDiEvent::Unknown {
                    method: method.to_string(),
                    params,
                },
            }
        }
        "browsingContext.contextCreated"
        | "browsingContext.contextDestroyed"
        | "browsingContext.navigationStarted"
        | "browsingContext.navigationAborted"
        | "browsingContext.navigationFailed"
        | "browsingContext.domContentLoaded"
        | "browsingContext.load"
        | "browsingContext.download" => {
            match serde_json::from_value::<BrowsingContextEvent>(
                json!({ "method": method, "params": params }),
            ) {
                Ok(e) => BiDiEvent::BrowsingContext(e),
                Err(_) => BiDiEvent::Unknown {
                    method: method.to_string(),
                    params,
                },
            }
        }
        _ => BiDiEvent::Unknown {
            method: method.to_string(),
            params,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_unknown() {
        let event = parse_event("some.unknownEvent", json!({"foo": "bar"}));
        matches!(event, BiDiEvent::Unknown { .. });
    }
}
