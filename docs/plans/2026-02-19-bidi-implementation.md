# WebDriver BiDi Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement WebDriver BiDi (bidirectional protocol) support in `thirtyfour` as a feature-gated, Rust-idiomatic extension; simultaneously remove the `selenium-manager` dependency.

**Architecture:** `BiDiSession` owns a persistent WebSocket connection managed by `tokio-tungstenite`. A background tokio task dispatches incoming frames — routing JSON-RPC responses to per-command `oneshot::Sender`s and broadcasting events through a `tokio::sync::broadcast` channel. Domain structs (`Network`, `Log`, `Script`, etc.) borrow `&BiDiSession` and delegate to `BiDiSession::send_command`.

**Tech Stack:** Rust/Tokio, `tokio-tungstenite` (new optional dep), `futures-util` (already present), `serde_json`, `tokio::sync::{broadcast, oneshot, Mutex}`, `std::sync::atomic::AtomicU64`.

---

## Important Context Before Starting

- Crate lives at `thirtyfour/thirtyfour/` (workspace root is `thirtyfour/`)
- Run all `cargo` commands from `thirtyfour/` (workspace root), or specify `--package thirtyfour`
- The error type is a newtype wrapper: add `BiDi(String)` inside the `webdriver_err!` macro call in `error.rs`
- The `webSocketUrl` returned by the server in session capabilities must be captured and stored in `SessionHandle`
- All BiDi code lives behind `#[cfg(feature = "bidi")]`
- Do NOT add `DashMap` — use `std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>` instead (never held across `.await`)
- Run `cargo check --package thirtyfour --features bidi` to verify compilation after each task
- Run `cargo test --package thirtyfour` to verify existing tests still pass

---

## Task 1: Remove selenium-manager + Add bidi feature flag

**Files:**
- Modify: `thirtyfour/thirtyfour/Cargo.toml`
- Modify: `thirtyfour/thirtyfour/src/lib.rs`

**Step 1: Write the test (compilation only — verify feature compiles)**

Create a placeholder file to verify the feature flag works. This will fail to compile until we add the dep.

```bash
# From thirtyfour/ (workspace root)
cargo check --package thirtyfour 2>&1 | tail -5
```

Expected: currently passes (no bidi feature yet).

**Step 2: Edit Cargo.toml**

In `thirtyfour/thirtyfour/Cargo.toml`, make these changes:

Remove `selenium-manager` from `default` features and from `[dependencies]`:

```toml
[features]
default = ["reqwest", "rustls-tls", "component"]
reqwest = ["dep:reqwest"]
rustls-tls = ["reqwest/rustls-tls"]
native-tls = ["reqwest/native-tls"]
tokio-multi-threaded = ["tokio/rt-multi-thread"]
component = ["thirtyfour-macros"]
debug_sync_quit = []
bidi = ["dep:tokio-tungstenite"]
```

In `[dependencies]`, remove:
```toml
libc = { version = "0.2", optional = true }
selenium-manager = { path = "../selenium/rust", optional = true }
```

Add:
```toml
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"], optional = true }
```

**Step 3: Fix lib.rs**

In `thirtyfour/thirtyfour/src/lib.rs`, remove the `selenium-manager`-gated exports:

Remove lines:
```rust
#[cfg(feature = "selenium-manager")]
pub use web_driver_process::{
    start_webdriver_process, start_webdriver_process_full, WebDriverProcess,
    WebDriverProcessBrowser, WebDriverProcessPort,
};
```

Remove lines:
```rust
#[cfg(feature = "selenium-manager")]
mod web_driver_process;
```

Also remove in `prelude` module:
```rust
#[cfg(feature = "selenium-manager")]
pub use crate::start_webdriver_process;
```

**Step 4: Verify compilation**

```bash
cargo check --package thirtyfour
```
Expected: PASS (no errors)

```bash
cargo check --package thirtyfour --features bidi
```
Expected: PASS (tokio-tungstenite resolves)

**Step 5: Run existing tests**

```bash
cargo test --package thirtyfour --lib 2>&1 | tail -20
```
Expected: existing tests pass

**Step 6: Commit**

```bash
git add thirtyfour/thirtyfour/Cargo.toml thirtyfour/thirtyfour/src/lib.rs
git commit -m "feat: remove selenium-manager dep, add bidi feature flag with tokio-tungstenite"
```

---

## Task 2: Capture webSocketUrl from session capabilities

**Files:**
- Modify: `thirtyfour/thirtyfour/src/session/create.rs`
- Modify: `thirtyfour/thirtyfour/src/session/handle.rs`
- Modify: `thirtyfour/thirtyfour/src/web_driver.rs`

**Background:** When a BiDi-capable browser accepts a `NewSession` request, the server response includes `value.capabilities.webSocketUrl`. Currently `start_session` discards the capabilities. We need to extract and store `webSocketUrl`.

**Step 1: Write the unit test**

In `thirtyfour/thirtyfour/src/session/create.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_websocket_url_present() {
        let resp_json = serde_json::json!({
            "sessionId": "abc",
            "value": {
                "sessionId": "abc",
                "capabilities": {
                    "browserName": "chrome",
                    "webSocketUrl": "ws://localhost:1234/session/abc/se/bidi"
                }
            }
        });
        let resp: SessionCreationResponse = serde_json::from_value(resp_json).unwrap();
        assert_eq!(
            resp.capabilities_websocket_url(),
            Some("ws://localhost:1234/session/abc/se/bidi")
        );
    }

    #[test]
    fn test_parse_websocket_url_absent() {
        let resp_json = serde_json::json!({
            "sessionId": "abc",
            "value": {
                "sessionId": "abc",
                "capabilities": { "browserName": "firefox" }
            }
        });
        let resp: SessionCreationResponse = serde_json::from_value(resp_json).unwrap();
        assert_eq!(resp.capabilities_websocket_url(), None);
    }
}
```

**Step 2: Run to verify FAIL**

```bash
cargo test --package thirtyfour --lib session::create::tests 2>&1 | tail -20
```
Expected: FAIL — `SessionCreationResponse` does not exist.

**Step 3: Implement `SessionCreationResponse` in `session/create.rs`**

Replace the existing private `ConnectionResp` / `ConnectionData` structs in `start_session` with a module-level named type that exposes `webSocketUrl`:

```rust
#[derive(Debug, Deserialize)]
struct SessionCapabilities {
    #[serde(rename = "webSocketUrl")]
    web_socket_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionCreationValue {
    #[serde(default, rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    capabilities: Option<SessionCapabilities>,
}

#[derive(Debug, Deserialize)]
struct SessionCreationResponse {
    #[serde(default, rename = "sessionId")]
    session_id: String,
    value: SessionCreationValue,
}

impl SessionCreationResponse {
    fn capabilities_websocket_url(&self) -> Option<&str> {
        self.value
            .capabilities
            .as_ref()
            .and_then(|c| c.web_socket_url.as_deref())
    }
}
```

Update `start_session` to return `(SessionId, Option<String>)` instead of `SessionId`:

```rust
pub async fn start_session(
    http_client: &dyn HttpClient,
    server_url: &Url,
    config: &WebDriverConfig,
    capabilities: Capabilities,
) -> WebDriverResult<(SessionId, Option<String>)> {
    // ... (existing retry logic unchanged) ...

    let resp: SessionCreationResponse = serde_json::from_value(v.body)?;
    let ws_url = resp.capabilities_websocket_url().map(String::from);
    let data = resp.value;
    let session_id = SessionId::from(if resp.session_id.is_empty() {
        data.session_id
    } else {
        resp.session_id
    });

    // Set default timeouts (unchanged).
    let request_data =
        Command::SetTimeouts(TimeoutConfiguration::default()).format_request(&session_id);
    run_webdriver_cmd(http_client, &request_data, server_url, config).await?;

    Ok((session_id, ws_url))
}
```

**Step 4: Update `SessionHandle` to store `websocket_url`**

In `thirtyfour/thirtyfour/src/session/handle.rs`:

Add field to the struct:
```rust
pub struct SessionHandle {
    pub client: Arc<dyn HttpClient>,
    server_url: Arc<Url>,
    session_id: SessionId,
    config: WebDriverConfig,
    quit: Arc<OnceCell<()>>,
    /// The WebSocket URL for BiDi connections (present only when the driver supports BiDi).
    pub websocket_url: Option<String>,
}
```

Update `new_with_config` to accept `websocket_url`:
```rust
pub(crate) fn new_with_config(
    client: Arc<dyn HttpClient>,
    server_url: impl IntoUrl,
    session_id: SessionId,
    config: WebDriverConfig,
    websocket_url: Option<String>,
) -> WebDriverResult<Self> {
    Ok(Self {
        client,
        server_url: Arc::new(server_url.into_url()?),
        session_id,
        config,
        quit: Arc::new(OnceCell::new()),
        websocket_url,
    })
}
```

Update `new` (public) to pass `None`:
```rust
pub fn new(
    client: Arc<dyn HttpClient>,
    server_url: impl IntoUrl,
    session_id: SessionId,
) -> WebDriverResult<Self> {
    Self::new_with_config(client, server_url, session_id, WebDriverConfig::default(), None)
}
```

Update `clone_with_config` to preserve `websocket_url`:
```rust
pub(crate) fn clone_with_config(self: &SessionHandle, config: WebDriverConfig) -> Self {
    Self {
        client: Arc::clone(&self.client),
        server_url: Arc::clone(&self.server_url),
        session_id: self.session_id.clone(),
        quit: Arc::clone(&self.quit),
        config,
        websocket_url: self.websocket_url.clone(),
    }
}
```

**Step 5: Update `WebDriver::new_with_config_and_client`**

In `thirtyfour/thirtyfour/src/web_driver.rs`:

```rust
pub async fn new_with_config_and_client<S, C>(
    server_url: S,
    capabilities: C,
    config: WebDriverConfig,
    client: impl HttpClient,
) -> WebDriverResult<Self>
where
    S: Into<String>,
    C: Into<Capabilities>,
{
    let capabilities = capabilities.into();
    let server_url = server_url
        .into()
        .parse()
        .map_err(|e| WebDriverError::ParseError(format!("invalid url: {e}")))?;

    let client = Arc::new(client);
    let (session_id, websocket_url) =
        start_session(client.as_ref(), &server_url, &config, capabilities).await?;

    let handle = SessionHandle::new_with_config(client, server_url, session_id, config, websocket_url)?;
    Ok(Self {
        handle: Arc::new(handle),
    })
}
```

**Step 6: Verify tests pass**

```bash
cargo test --package thirtyfour --lib 2>&1 | tail -20
```
Expected: `test session::create::tests::test_parse_websocket_url_present ... ok` and `test_parse_websocket_url_absent ... ok`

**Step 7: Commit**

```bash
git add thirtyfour/thirtyfour/src/session/create.rs \
        thirtyfour/thirtyfour/src/session/handle.rs \
        thirtyfour/thirtyfour/src/web_driver.rs
git commit -m "feat: capture webSocketUrl from session capabilities and store on SessionHandle"
```

---

## Task 3: Add `WebDriverError::BiDi` variant

**Files:**
- Modify: `thirtyfour/thirtyfour/src/error.rs`

**Step 1: Write the failing test**

In `thirtyfour/thirtyfour/src/error.rs`, add at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bidi_error_variant() {
        let err = WebDriverError::BiDi("connection refused".to_string());
        assert!(err.to_string().contains("BiDi error: connection refused"));
    }
}
```

**Step 2: Run to verify FAIL**

```bash
cargo test --package thirtyfour --lib error::tests 2>&1 | tail -10
```
Expected: FAIL — `BiDi` variant does not exist.

**Step 3: Add the `BiDi` variant**

In `thirtyfour/thirtyfour/src/error.rs`, inside the `webdriver_err!` macro call, add after the last variant (`SessionCreateError`):

```rust
        #[error("BiDi error: {0}")]
        BiDi(String),
```

**Step 4: Add `From<tokio_tungstenite::tungstenite::Error>` for when `bidi` feature is active**

At the bottom of `error.rs`, after the existing `From` impls:

```rust
#[cfg(feature = "bidi")]
impl From<tokio_tungstenite::tungstenite::Error> for WebDriverError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        WebDriverError::BiDi(err.to_string())
    }
}
```

**Step 5: Run test to verify PASS**

```bash
cargo test --package thirtyfour --lib error::tests --features bidi 2>&1 | tail -10
```
Expected: PASS

**Step 6: Commit**

```bash
git add thirtyfour/thirtyfour/src/error.rs
git commit -m "feat: add WebDriverError::BiDi variant and From<tungstenite::Error>"
```

---

## Task 4: Create `BiDiSession` core — WebSocket + dispatch loop

**Files:**
- Create: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`
- Modify: `thirtyfour/thirtyfour/src/extensions/mod.rs`

**Step 1: Write the failing test**

Create `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs` with this test-only module at the bottom. This will compile only under `#[cfg(test)]` using a mock, so it fails now because the file doesn't exist:

```bash
cargo check --package thirtyfour --features bidi 2>&1 | grep "bidi"
```
Expected: no `bidi` module yet.

**Step 2: Register the module**

In `thirtyfour/thirtyfour/src/extensions/mod.rs`, add:

```rust
/// Extensions for working with Firefox Addons.
pub mod addons;
/// Extensions for Chrome Devtools Protocol
pub mod cdp;
// ElementQuery and ElementWaiter interfaces.
pub mod query;
/// WebDriver BiDi bidirectional protocol support.
#[cfg(feature = "bidi")]
pub mod bidi;
```

**Step 3: Create `extensions/bidi/mod.rs`**

```rust
//! WebDriver BiDi bidirectional protocol support.
//!
//! Enable with the `bidi` cargo feature.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{broadcast, oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::error::{WebDriverError, WebDriverResult};

pub mod session;
pub mod log;
pub mod network;
pub mod browsing_context;
pub mod script;
pub mod browser;
pub mod console;
pub mod input;
pub mod permissions;
pub mod storage;
pub mod webextension;
pub mod emulation;
pub mod cdp;

pub use session::Session;
pub use log::Log;
pub use network::{Network, NetworkEvent};
pub use browsing_context::{BrowsingContext, BrowsingContextEvent};
pub use script::{Script, ScriptEvent};
pub use browser::Browser;
pub use console::Console;
pub use input::Input;
pub use permissions::Permissions;
pub use storage::Storage;
pub use webextension::WebExtension;
pub use emulation::Emulation;
pub use cdp::Cdp;

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
    Unknown { method: String, params: Value },
}

/// A live WebDriver BiDi session over a WebSocket connection.
///
/// Obtain one by calling [`WebDriver::bidi_connect`][crate::WebDriver::bidi_connect].
pub struct BiDiSession {
    /// Sends frames to the WebSocket.
    ws_sink: Arc<Mutex<futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>
        >,
        Message,
    >>>,
    /// Auto-incrementing JSON-RPC command id.
    command_id: Arc<AtomicU64>,
    /// In-flight commands waiting for a response. Never held across `.await`.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WebDriverResult<Value>>>>>,
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
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<WebDriverResult<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

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
            ws_sink: Arc::new(Mutex::new(sink)),
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

        {
            let mut sink = self.ws_sink.lock().unwrap();
            // We cannot .await inside a std::sync::MutexGuard, so we use
            // try_send (non-async) from tungstenite via a blocking send approach.
            // Use a channel to send to the async sink without holding the mutex.
            drop(sink); // Release lock immediately
        }

        // Use a dedicated task to send the WebSocket message so we don't hold
        // a std::sync::MutexGuard across an await point.
        let sink_clone = Arc::clone(&self.ws_sink);
        tokio::spawn(async move {
            let mut sink = sink_clone.lock().unwrap();
            let _ = sink.send(Message::Text(text.into())).await;
        }).await.map_err(|e| WebDriverError::BiDi(format!("send task failed: {e}")))?;

        rx.await
            .map_err(|_| WebDriverError::BiDi("response channel closed".to_string()))?
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
                Err(_) => BiDiEvent::Unknown { method: method.to_string(), params },
            }
        }
        "log.entryAdded" => {
            match serde_json::from_value::<log::LogEvent>(params.clone()) {
                Ok(e) => {
                    // If it's a console entry, also emit as Console variant.
                    BiDiEvent::Log(e)
                }
                Err(_) => BiDiEvent::Unknown { method: method.to_string(), params },
            }
        }
        "script.realmCreated" | "script.realmDestroyed" => {
            match serde_json::from_value::<ScriptEvent>(
                json!({ "method": method, "params": params }),
            ) {
                Ok(e) => BiDiEvent::Script(e),
                Err(_) => BiDiEvent::Unknown { method: method.to_string(), params },
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
                Err(_) => BiDiEvent::Unknown { method: method.to_string(), params },
            }
        }
        _ => BiDiEvent::Unknown { method: method.to_string(), params },
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
```

**Step 4: Run to verify module compiles**

```bash
cargo check --package thirtyfour --features bidi 2>&1 | tail -30
```
Expected: errors about missing submodule files (that's expected — we'll create them in subsequent tasks).

**Step 5: Create stub files for all domain submodules**

Create minimal stub files so the module compiles. We'll flesh them out in later tasks.

Create `thirtyfour/thirtyfour/src/extensions/bidi/session.rs`:
```rust
use crate::error::WebDriverResult;
use super::BiDiSession;

/// BiDi `session` domain accessor.
pub struct Session<'a> {
    session: &'a BiDiSession,
}

impl<'a> Session<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
    }

    /// Subscribe to events for the given event methods and browsing contexts.
    /// Pass `events = &[]` and `contexts = &[]` to subscribe globally.
    pub async fn subscribe(
        &self,
        events: &[&str],
        contexts: &[&str],
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "events": events,
            "contexts": contexts,
        });
        self.session.send_command("session.subscribe", params).await?;
        Ok(())
    }

    /// Unsubscribe from events.
    pub async fn unsubscribe(
        &self,
        events: &[&str],
        contexts: &[&str],
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "events": events,
            "contexts": contexts,
        });
        self.session.send_command("session.unsubscribe", params).await?;
        Ok(())
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/log.rs`:
```rust
use serde::Deserialize;
use tokio::sync::broadcast;
use super::{BiDiEvent, BiDiSession};
use crate::error::WebDriverResult;

/// A single log entry received from the browser.
#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub text: Option<String>,
    pub timestamp: Option<u64>,
    pub source: Option<LogSource>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Source information for a log entry.
#[derive(Debug, Clone, Deserialize)]
pub struct LogSource {
    pub realm: Option<String>,
    pub context: Option<String>,
}

/// Log domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LogEvent {
    EntryAdded(LogEntry),
}

/// Console domain event (re-exported from log).
#[derive(Debug, Clone)]
pub struct ConsoleEvent(pub LogEntry);

/// BiDi `log` domain accessor.
pub struct Log<'a> {
    session: &'a BiDiSession,
}

impl<'a> Log<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
    }

    /// Subscribe to `log.entryAdded` events. Returns a broadcast receiver.
    /// You must first call `bidi.session().subscribe(&["log.entryAdded"], &[]).await?`
    /// to tell the browser to start sending these events.
    pub fn subscribe(&self) -> broadcast::Receiver<BiDiEvent> {
        self.session.subscribe_events()
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/network.rs`:
```rust
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use super::{BiDiEvent, BiDiSession};
use crate::error::WebDriverResult;

/// The phase at which to intercept network requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterceptPhase {
    BeforeRequestSent,
    ResponseStarted,
    AuthRequired,
}

/// A network request intercepted by BiDi.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkRequest {
    pub method: String,
    pub url: String,
    pub headers: Option<Vec<Header>>,
    pub body_size: Option<u64>,
}

/// A network response.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkResponse {
    pub url: String,
    pub status: u16,
    pub headers: Option<Vec<Header>>,
    pub body_size: Option<u64>,
}

/// An HTTP header.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Header {
    pub name: String,
    pub value: HeaderValue,
}

/// The value of an HTTP header.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum HeaderValue {
    String(StringValue),
    Base64(Base64Value),
}

/// A string header value.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StringValue {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

/// A base64-encoded header value.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Base64Value {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

/// Network domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum NetworkEvent {
    #[serde(rename = "network.beforeRequestSent")]
    BeforeRequestSent(BeforeRequestSentParams),
    #[serde(rename = "network.responseStarted")]
    ResponseStarted(ResponseStartedParams),
    #[serde(rename = "network.responseCompleted")]
    ResponseCompleted(ResponseCompletedParams),
    #[serde(rename = "network.fetchError")]
    FetchError(FetchErrorParams),
    #[serde(rename = "network.authRequired")]
    AuthRequired(AuthRequiredParams),
}

#[derive(Debug, Clone, Deserialize)]
pub struct BeforeRequestSentParams {
    pub request_id: String,
    pub request: NetworkRequest,
    pub context: Option<String>,
    pub intercepts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseStartedParams {
    pub request_id: String,
    pub request: NetworkRequest,
    pub response: NetworkResponse,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseCompletedParams {
    pub request_id: String,
    pub request: NetworkRequest,
    pub response: NetworkResponse,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FetchErrorParams {
    pub request_id: String,
    pub error_text: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthRequiredParams {
    pub request_id: String,
    pub request: NetworkRequest,
    pub context: Option<String>,
}

/// BiDi `network` domain accessor.
pub struct Network<'a> {
    session: &'a BiDiSession,
}

impl<'a> Network<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
    }

    /// Add a network intercept for the given phases. Returns the intercept id.
    pub async fn add_intercept(&self, phases: &[InterceptPhase]) -> WebDriverResult<String> {
        let params = serde_json::json!({ "phases": phases });
        let result = self.session.send_command("network.addIntercept", params).await?;
        let intercept_id = result["intercept"]
            .as_str()
            .ok_or_else(|| crate::error::WebDriverError::BiDi(
                "missing 'intercept' in addIntercept response".to_string()
            ))?
            .to_string();
        Ok(intercept_id)
    }

    /// Remove a previously added intercept.
    pub async fn remove_intercept(&self, intercept_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "intercept": intercept_id });
        self.session.send_command("network.removeIntercept", params).await?;
        Ok(())
    }

    /// Continue a blocked request without modification.
    pub async fn continue_request(&self, request_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "request": request_id });
        self.session.send_command("network.continueRequest", params).await?;
        Ok(())
    }

    /// Continue a blocked response without modification.
    pub async fn continue_response(&self, request_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "request": request_id });
        self.session.send_command("network.continueResponse", params).await?;
        Ok(())
    }

    /// Fail a blocked request.
    pub async fn fail_request(&self, request_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "request": request_id });
        self.session.send_command("network.failRequest", params).await?;
        Ok(())
    }

    /// Provide a mock response for a blocked request.
    pub async fn provide_response(
        &self,
        request_id: &str,
        status_code: u16,
        body: Option<&str>,
    ) -> WebDriverResult<()> {
        let mut params = serde_json::json!({
            "request": request_id,
            "statusCode": status_code,
        });
        if let Some(b) = body {
            params["body"] = serde_json::json!({ "type": "string", "value": b });
        }
        self.session.send_command("network.provideResponse", params).await?;
        Ok(())
    }

    /// Subscribe to network events. Returns a broadcast receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<BiDiEvent> {
        self.session.subscribe_events()
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/browsing_context.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::BiDiSession;
use crate::error::WebDriverResult;

/// How to create a new browsing context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateType {
    Tab,
    Window,
}

/// Browsing context domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum BrowsingContextEvent {
    #[serde(rename = "browsingContext.contextCreated")]
    ContextCreated(ContextInfo),
    #[serde(rename = "browsingContext.contextDestroyed")]
    ContextDestroyed(ContextInfo),
    #[serde(rename = "browsingContext.navigationStarted")]
    NavigationStarted(NavigationInfo),
    #[serde(rename = "browsingContext.domContentLoaded")]
    DomContentLoaded(NavigationInfo),
    #[serde(rename = "browsingContext.load")]
    Load(NavigationInfo),
    #[serde(rename = "browsingContext.navigationAborted")]
    NavigationAborted(NavigationInfo),
    #[serde(rename = "browsingContext.navigationFailed")]
    NavigationFailed(NavigationInfo),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextInfo {
    pub context: String,
    pub url: Option<String>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NavigationInfo {
    pub context: String,
    pub navigation: Option<String>,
    pub url: String,
}

/// BiDi `browsingContext` domain accessor.
pub struct BrowsingContext<'a> {
    session: &'a BiDiSession,
}

impl<'a> BrowsingContext<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
    }

    /// Get the tree of currently open browsing contexts.
    pub async fn get_tree(&self) -> WebDriverResult<serde_json::Value> {
        self.session.send_command("browsingContext.getTree", serde_json::json!({})).await
    }

    /// Create a new browsing context (tab or window). Returns the context id.
    pub async fn create(&self, kind: CreateType) -> WebDriverResult<String> {
        let params = serde_json::json!({ "type": kind });
        let result = self.session.send_command("browsingContext.create", params).await?;
        let ctx = result["context"]
            .as_str()
            .ok_or_else(|| crate::error::WebDriverError::BiDi(
                "missing 'context' in browsingContext.create response".to_string()
            ))?
            .to_string();
        Ok(ctx)
    }

    /// Close a browsing context.
    pub async fn close(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("browsingContext.close", params).await?;
        Ok(())
    }

    /// Navigate a browsing context to a URL. Returns the navigation id.
    pub async fn navigate(&self, context: &str, url: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({ "context": context, "url": url });
        let result = self.session.send_command("browsingContext.navigate", params).await?;
        Ok(result["navigation"].as_str().unwrap_or("").to_string())
    }

    /// Reload the current page in a browsing context.
    pub async fn reload(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("browsingContext.reload", params).await?;
        Ok(())
    }

    /// Activate (focus) a browsing context.
    pub async fn activate(&self, context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context });
        self.session.send_command("browsingContext.activate", params).await?;
        Ok(())
    }

    /// Set the viewport size of a browsing context.
    pub async fn set_viewport(
        &self,
        context: &str,
        width: u32,
        height: u32,
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "context": context,
            "viewport": { "width": width, "height": height }
        });
        self.session.send_command("browsingContext.setViewport", params).await?;
        Ok(())
    }

    /// Capture a screenshot of a browsing context. Returns base64-encoded PNG bytes.
    pub async fn capture_screenshot(&self, context: &str) -> WebDriverResult<String> {
        let params = serde_json::json!({ "context": context });
        let result =
            self.session.send_command("browsingContext.captureScreenshot", params).await?;
        Ok(result["data"].as_str().unwrap_or("").to_string())
    }

    /// Traverse browsing history by delta (negative = back, positive = forward).
    pub async fn traverse_history(&self, context: &str, delta: i32) -> WebDriverResult<()> {
        let params = serde_json::json!({ "context": context, "delta": delta });
        self.session.send_command("browsingContext.traverseHistory", params).await?;
        Ok(())
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/script.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::BiDiSession;
use crate::error::WebDriverResult;

/// Script domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum ScriptEvent {
    #[serde(rename = "script.realmCreated")]
    RealmCreated(RealmInfo),
    #[serde(rename = "script.realmDestroyed")]
    RealmDestroyed(RealmDestroyedParams),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealmInfo {
    pub realm: String,
    pub origin: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealmDestroyedParams {
    pub realm: String,
}

/// `script.evaluate` result.
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateResult {
    #[serde(rename = "type")]
    pub kind: String,
    pub result: Option<serde_json::Value>,
    #[serde(rename = "exceptionDetails")]
    pub exception_details: Option<serde_json::Value>,
}

/// BiDi `script` domain accessor.
pub struct Script<'a> {
    session: &'a BiDiSession,
}

impl<'a> Script<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
    pub async fn add_preload_script(
        &self,
        function_declaration: &str,
    ) -> WebDriverResult<String> {
        let params = serde_json::json!({
            "functionDeclaration": function_declaration,
        });
        let result = self.session.send_command("script.addPreloadScript", params).await?;
        Ok(result["script"].as_str().unwrap_or("").to_string())
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
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/browser.rs`:
```rust
use serde::Deserialize;
use super::BiDiSession;
use crate::error::WebDriverResult;

/// Information about a browser user context.
#[derive(Debug, Clone, Deserialize)]
pub struct UserContextInfo {
    #[serde(rename = "userContext")]
    pub user_context: String,
}

/// BiDi `browser` domain accessor.
pub struct Browser<'a> {
    session: &'a BiDiSession,
}

impl<'a> Browser<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
    }

    /// Close the browser.
    pub async fn close(&self) -> WebDriverResult<()> {
        self.session.send_command("browser.close", serde_json::json!({})).await?;
        Ok(())
    }

    /// Create a new user context (browser profile). Returns the user context id.
    pub async fn create_user_context(&self) -> WebDriverResult<String> {
        let result = self
            .session
            .send_command("browser.createUserContext", serde_json::json!({}))
            .await?;
        Ok(result["userContext"].as_str().unwrap_or("").to_string())
    }

    /// Remove (delete) a user context.
    pub async fn remove_user_context(&self, user_context: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "userContext": user_context });
        self.session.send_command("browser.removeUserContext", params).await?;
        Ok(())
    }

    /// Get all user contexts.
    pub async fn get_user_contexts(&self) -> WebDriverResult<Vec<UserContextInfo>> {
        let result = self
            .session
            .send_command("browser.getUserContexts", serde_json::json!({}))
            .await?;
        let infos: Vec<UserContextInfo> = serde_json::from_value(result["userContexts"].clone())
            .map_err(|e| crate::error::WebDriverError::BiDi(format!("parse error: {e}")))?;
        Ok(infos)
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/console.rs`:
```rust
use tokio::sync::broadcast;
use super::{BiDiEvent, BiDiSession};
pub use super::log::ConsoleEvent;

/// BiDi `console` domain accessor (thin wrapper over log.entryAdded events with console source).
pub struct Console<'a> {
    session: &'a BiDiSession,
}

impl<'a> Console<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
    }

    /// Subscribe to all BiDi events (caller should filter for log.entryAdded + console source).
    pub fn subscribe(&self) -> broadcast::Receiver<BiDiEvent> {
        self.session.subscribe_events()
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/input.rs`:
```rust
use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `input` domain accessor.
pub struct Input<'a> {
    session: &'a BiDiSession,
}

impl<'a> Input<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/permissions.rs`:
```rust
use super::BiDiSession;
use crate::error::WebDriverResult;

/// Permission state.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
}

/// BiDi `permissions` domain accessor.
pub struct Permissions<'a> {
    session: &'a BiDiSession,
}

impl<'a> Permissions<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/storage.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::BiDiSession;
use crate::error::WebDriverResult;

/// A BiDi cookie.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cookie {
    pub name: String,
    pub value: CookieValue,
    pub domain: String,
    pub path: Option<String>,
    pub secure: Option<bool>,
    #[serde(rename = "httpOnly")]
    pub http_only: Option<bool>,
    #[serde(rename = "sameSite")]
    pub same_site: Option<String>,
    pub expiry: Option<u64>,
}

/// Cookie value (string or base64).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CookieValue {
    String { #[serde(rename = "type")] kind: String, value: String },
}

/// BiDi `storage` domain accessor.
pub struct Storage<'a> {
    session: &'a BiDiSession,
}

impl<'a> Storage<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
        serde_json::from_value(result["cookies"].clone())
            .map_err(|e| crate::error::WebDriverError::BiDi(format!("parse error: {e}")))
    }

    /// Set a cookie in the given partition.
    pub async fn set_cookie(&self, cookie: Cookie) -> WebDriverResult<()> {
        let params = serde_json::json!({ "cookie": cookie });
        self.session.send_command("storage.setCookie", params).await?;
        Ok(())
    }

    /// Delete cookies matching the given filter.
    pub async fn delete_cookies(
        &self,
        filter: Option<serde_json::Value>,
    ) -> WebDriverResult<()> {
        let mut params = serde_json::json!({});
        if let Some(f) = filter {
            params["filter"] = f;
        }
        self.session.send_command("storage.deleteCookies", params).await?;
        Ok(())
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/webextension.rs`:
```rust
use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `webExtension` domain accessor.
pub struct WebExtension<'a> {
    session: &'a BiDiSession,
}

impl<'a> WebExtension<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
        Ok(result["extension"].as_str().unwrap_or("").to_string())
    }

    /// Uninstall a web extension by id.
    pub async fn uninstall(&self, extension_id: &str) -> WebDriverResult<()> {
        let params = serde_json::json!({ "extension": extension_id });
        self.session.send_command("webExtension.uninstall", params).await?;
        Ok(())
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/emulation.rs`:
```rust
use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi `emulation` domain accessor.
pub struct Emulation<'a> {
    session: &'a BiDiSession,
}

impl<'a> Emulation<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
    pub async fn set_device_posture(
        &self,
        context: &str,
        posture: &str,
    ) -> WebDriverResult<()> {
        let params = serde_json::json!({
            "context": context,
            "posture": posture,
        });
        self.session.send_command("emulation.setDevicePosture", params).await?;
        Ok(())
    }
}
```

Create `thirtyfour/thirtyfour/src/extensions/bidi/cdp.rs`:
```rust
use super::BiDiSession;
use crate::error::WebDriverResult;

/// BiDi CDP passthrough domain accessor.
pub struct Cdp<'a> {
    session: &'a BiDiSession,
}

impl<'a> Cdp<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self { session }
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
```

**Step 6: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi 2>&1 | tail -30
```
Expected: PASS (possibly with warnings about unused fields/variants; those are OK for now)

**Step 7: Run unit tests**

```bash
cargo test --package thirtyfour --features bidi --lib 2>&1 | tail -20
```
Expected: `test extensions::bidi::tests::test_parse_event_unknown ... ok`

**Step 8: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/ \
        thirtyfour/thirtyfour/src/extensions/mod.rs
git commit -m "feat(bidi): add BiDiSession core + all domain stubs (feature-gated)"
```

---

## Task 5: Fix `send_command` WebSocket send (avoid mutex across await)

**Background:** The `send_command` implementation in Task 4 has a subtle problem: `std::sync::Mutex` cannot be held across `.await`. The `ws_sink` needs a redesign. The correct approach is to use `tokio::sync::Mutex` for the sink (Tokio's async mutex is safe across `.await`).

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Write the test**

Add to the tests block in `mod.rs`:

```rust
#[test]
fn test_bidi_session_is_send() {
    fn assert_send<T: Send>() {}
    // BiDiSession must be Send so it can be used across tokio tasks.
    // This is a compile-time check.
    assert_send::<BiDiSession>();
}
```

**Step 2: Run to verify FAIL**

```bash
cargo test --package thirtyfour --features bidi --lib extensions::bidi::tests::test_bidi_session_is_send 2>&1 | tail -20
```
Expected: FAIL or compile error if `BiDiSession` is not `Send`.

**Step 3: Rewrite the sink field to use `tokio::sync::Mutex`**

In `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`, replace:

```rust
use std::sync::Mutex;
```

Change the import to use both:
```rust
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex as TokioMutex;
```

Change the `ws_sink` field type in the struct:
```rust
ws_sink: Arc<TokioMutex<futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>
    >,
    Message,
>>>,
```

Keep `pending` using `StdMutex<HashMap<...>>` (never held across await).

Update `BiDiSession::connect` to use `TokioMutex::new(sink)`.

Rewrite `send_command` without spawning a separate task:

```rust
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

    rx.await
        .map_err(|_| WebDriverError::BiDi("response channel closed".to_string()))?
}
```

**Step 4: Verify**

```bash
cargo check --package thirtyfour --features bidi 2>&1 | tail -10
cargo test --package thirtyfour --features bidi --lib 2>&1 | tail -20
```
Expected: PASS

**Step 5: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "fix(bidi): use tokio::sync::Mutex for ws_sink to safely await send"
```

---

## Task 6: Add `bidi_connect` to `WebDriver`

**Files:**
- Modify: `thirtyfour/thirtyfour/src/web_driver.rs`

**Step 1: Write the test (compile-time)**

In `thirtyfour/thirtyfour/src/web_driver.rs`, add at the end of the `#[cfg(test)]` block (or create one):

```rust
#[cfg(all(test, feature = "bidi"))]
mod bidi_tests {
    use super::*;

    #[test]
    fn test_bidi_connect_exists() {
        // Verify the method signature compiles. We can't call it without a live driver.
        let _: fn(&WebDriver) -> _ = |d| d.bidi_connect();
    }
}
```

**Step 2: Run to verify FAIL**

```bash
cargo test --package thirtyfour --features bidi --lib web_driver::bidi_tests 2>&1 | tail -10
```
Expected: FAIL — `bidi_connect` does not exist on `WebDriver`.

**Step 3: Add `bidi_connect` to `WebDriver`**

In `thirtyfour/thirtyfour/src/web_driver.rs`, add after the `leak` method:

```rust
#[cfg(feature = "bidi")]
/// Connect to the WebDriver BiDi channel.
///
/// Requires the browser to have been started with BiDi capabilities enabled.
/// For Chrome, set `"webSocketUrl": true` in capabilities.
/// For Firefox, BiDi is enabled by default in supported versions.
///
/// Returns a [`BiDiSession`][crate::extensions::bidi::BiDiSession] that can be
/// used to interact with the browser via the BiDi protocol.
///
/// # Errors
///
/// Returns `WebDriverError::BiDi` if:
/// - The browser did not return a `webSocketUrl` in session capabilities
/// - The WebSocket connection fails
pub async fn bidi_connect(&self) -> crate::error::WebDriverResult<crate::extensions::bidi::BiDiSession> {
    let ws_url = self.handle.websocket_url.as_deref().ok_or_else(|| {
        crate::prelude::WebDriverError::BiDi(
            "No webSocketUrl in session capabilities. \
             Enable BiDi in your browser capabilities \
             (e.g., for Chrome: set 'webSocketUrl: true')."
                .to_string(),
        )
    })?;
    crate::extensions::bidi::BiDiSession::connect(ws_url).await
}
```

**Step 4: Verify**

```bash
cargo check --package thirtyfour --features bidi 2>&1 | tail -10
cargo test --package thirtyfour --features bidi --lib 2>&1 | tail -20
```
Expected: PASS

**Step 5: Commit**

```bash
git add thirtyfour/thirtyfour/src/web_driver.rs
git commit -m "feat(bidi): add WebDriver::bidi_connect() behind bidi feature flag"
```

---

## Task 7: Export BiDi types in `lib.rs` and add example

**Files:**
- Modify: `thirtyfour/thirtyfour/src/lib.rs`
- Create: `thirtyfour/thirtyfour/examples/bidi_network_intercept.rs`
- Modify: `thirtyfour/thirtyfour/Cargo.toml` (add example entry)

**Step 1: Export bidi module re-exports in `lib.rs`**

In `thirtyfour/thirtyfour/src/lib.rs`, below the `pub mod extensions;` line, add:

```rust
#[cfg(feature = "bidi")]
/// Re-export BiDi types for convenience.
pub use extensions::bidi::{BiDiEvent, BiDiSession};
```

**Step 2: Create the example file**

Create `thirtyfour/thirtyfour/examples/bidi_network_intercept.rs`:

```rust
//! Example: intercept network requests using WebDriver BiDi.
//!
//! Requires Chrome 115+ started with BiDi support:
//! ```json
//! { "webSocketUrl": true }
//! ```
//!
//! Run with:
//! ```
//! cargo run --example bidi_network_intercept --features bidi
//! ```

use thirtyfour::extensions::bidi::{network::InterceptPhase, BiDiEvent, NetworkEvent};
use thirtyfour::prelude::*;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    // Enable BiDi by requesting webSocketUrl in capabilities.
    let mut caps = DesiredCapabilities::chrome();
    caps.insert_base_capability("webSocketUrl".to_string(), serde_json::Value::Bool(true));

    let driver = WebDriver::new("http://localhost:4444", caps).await?;

    // Connect to the BiDi channel.
    let bidi = driver.bidi_connect().await?;

    // Tell the browser we want network events.
    bidi.session()
        .subscribe(&["network.beforeRequestSent"], &[])
        .await?;

    // Add a network intercept for the beforeRequestSent phase.
    let intercept_id = bidi
        .network()
        .add_intercept(&[InterceptPhase::BeforeRequestSent])
        .await?;

    // Start listening to events.
    let mut rx = bidi.subscribe_events();

    // Navigate — this triggers network events.
    driver.goto("https://example.com").await?;

    // Read a few events.
    for _ in 0..3 {
        match rx.recv().await {
            Ok(BiDiEvent::Network(NetworkEvent::BeforeRequestSent(e))) => {
                println!(
                    "Intercepted: {} {}",
                    e.request.method, e.request.url
                );
                // Continue the request.
                bidi.network().continue_request(&e.request_id).await?;
            }
            Ok(event) => println!("Other event: {event:?}"),
            Err(e) => {
                println!("Recv error: {e}");
                break;
            }
        }
    }

    // Clean up.
    bidi.network().remove_intercept(&intercept_id).await?;
    driver.quit().await?;

    Ok(())
}
```

**Step 3: Add the example to `Cargo.toml`**

In `thirtyfour/thirtyfour/Cargo.toml`, add:

```toml
[[example]]
name = "bidi_network_intercept"
path = "examples/bidi_network_intercept.rs"
required-features = ["bidi"]
```

**Step 4: Verify compilation of example**

```bash
cargo check --package thirtyfour --features bidi --example bidi_network_intercept 2>&1 | tail -20
```
Expected: PASS

**Step 5: Run all lib tests**

```bash
cargo test --package thirtyfour --features bidi --lib 2>&1 | tail -30
cargo test --package thirtyfour --lib 2>&1 | tail -20
```
Expected: All tests pass with and without the `bidi` feature.

**Step 6: Commit**

```bash
git add thirtyfour/thirtyfour/src/lib.rs \
        thirtyfour/thirtyfour/examples/bidi_network_intercept.rs \
        thirtyfour/thirtyfour/Cargo.toml
git commit -m "feat(bidi): export BiDi types, add bidi_network_intercept example"
```

---

## Task 8: Final compilation check and test sweep

**Step 1: Check without bidi feature (no regressions)**

```bash
cargo check --package thirtyfour 2>&1 | tail -10
```
Expected: PASS

**Step 2: Check with bidi feature**

```bash
cargo check --package thirtyfour --features bidi 2>&1 | tail -10
```
Expected: PASS

**Step 3: Check all features**

```bash
cargo check --package thirtyfour --all-features 2>&1 | tail -10
```
Expected: PASS

**Step 4: Run full test suite (lib tests only — integration tests need a live browser)**

```bash
cargo test --package thirtyfour --lib 2>&1 | tail -30
cargo test --package thirtyfour --lib --features bidi 2>&1 | tail -30
```
Expected: All passing.

**Step 5: Verify cargo clippy (optional but recommended)**

```bash
cargo clippy --package thirtyfour --features bidi -- -D warnings 2>&1 | tail -30
```
Fix any `deny`-level warnings before committing.

**Step 6: Final commit if any fixes were needed**

```bash
git add -p
git commit -m "fix(bidi): address clippy warnings"
```

---

## Notes for Implementer

### How `webSocketUrl` is enabled per browser

**Chrome/Chromium:**
```rust
let mut caps = DesiredCapabilities::chrome();
caps.insert_base_capability("webSocketUrl".to_string(), serde_json::Value::Bool(true));
```

**Firefox (geckodriver >= 0.32 + Firefox 119+):**
BiDi is auto-enabled; `webSocketUrl` is returned without extra configuration.

### Testing without a live browser

All unit tests in this implementation are pure Rust logic tests (parsing, type checks). They run with `cargo test --lib`. Integration tests that open a real browser are marked `#[ignore]` and require Docker:

```bash
# Start a BiDi-capable Chrome container
docker run -d -p 4444:4444 --shm-size="1.8g" selenium/standalone-chrome:latest

# Run integration tests
cargo test --package thirtyfour --features bidi -- --ignored
```

### Wire protocol reminder

```
Command:  {"id": 1, "method": "network.addIntercept", "params": {"phases": ["beforeRequestSent"]}}
Success:  {"id": 1, "type": "success", "result": {"intercept": "abc123"}}
Error:    {"id": 1, "type": "error", "error": "unknown command", "message": "..."}
Event:    {"type": "event", "method": "network.beforeRequestSent", "params": {...}}
```
