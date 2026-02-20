# BiDi Enhancements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enhance WebDriver BiDi with typed event channels, flexible timeouts, builder pattern, connection state tracking, and enhanced tracing.

**Architecture:** Dual-channel event system (unified + typed), three-level timeout configuration (none/session/command), lazy-initialized Arc<OnceLock> channels, Arc<AtomicBool> connection tracking.

**Tech Stack:** Rust/Tokio, std::sync::OnceLock, std::sync::atomic::AtomicBool, std::time::Duration, tokio::sync::broadcast, tracing crate.

---

## Important Context Before Starting

- Crate lives at `thirtyfour/thirtyfour/` (workspace root is `thirtyfour/`)
- Run all `cargo` commands from `thirtyfour/` (workspace root), or specify `--package thirtyfour`
- All BiDi code lives behind `#[cfg(feature = "bidi")]`
- Run `cargo check --package thirtyfour --features bidi` after each task
- Run `cargo test --package thirtyfour --lib --features bidi` after each task
- Zero breaking changes - all additions are opt-in
- Follow TDD: write tests first, verify they fail, implement, verify they pass

---

## Task 1: Add BiDiSessionBuilder with tests

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Add imports**

Add to imports at top of file:

```rust
use std::time::Duration;
```

**Step 2: Write failing tests**

Add to bottom of file in `#[cfg(test)]` section:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_builder_defaults() {
        let builder = BiDiSessionBuilder::new();
        assert_eq!(builder.event_channel_capacity, 256);
        assert_eq!(builder.command_timeout, None);
    }

    #[test]
    fn test_builder_configuration() {
        let builder = BiDiSessionBuilder::new()
            .command_timeout(Duration::from_secs(10))
            .event_channel_capacity(512);
        assert_eq!(builder.command_timeout, Some(Duration::from_secs(10)));
        assert_eq!(builder.event_channel_capacity, 512);
    }

    #[test]
    fn test_builder_default_trait() {
        let builder = BiDiSessionBuilder::default();
        assert_eq!(builder.event_channel_capacity, 256);
        assert_eq!(builder.command_timeout, None);
    }
}
```

**Step 3: Run tests to verify failure**

```bash
cargo test --package thirtyfour --lib --features bidi extensions::bidi::tests
```

Expected: FAIL - `BiDiSessionBuilder` does not exist

**Step 4: Implement BiDiSessionBuilder**

Add before `BiDiSession` struct:

```rust
/// Builder for configuring a `BiDiSession` before connecting.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use thirtyfour::prelude::*;
///
/// # async fn example(driver: &WebDriver) -> WebDriverResult<()> {
/// let bidi = driver.bidi_connect_builder()
///     .command_timeout(Duration::from_secs(30))
///     .event_channel_capacity(512)
///     .connect()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct BiDiSessionBuilder {
    event_channel_capacity: usize,
    command_timeout: Option<Duration>,
}

impl BiDiSessionBuilder {
    /// Create a new builder with default settings.
    ///
    /// Defaults:
    /// - `event_channel_capacity`: 256
    /// - `command_timeout`: None (commands wait indefinitely)
    pub fn new() -> Self {
        Self {
            event_channel_capacity: 256,
            command_timeout: None,
        }
    }

    /// Set the broadcast channel capacity for events (default: 256).
    ///
    /// This controls how many events can be buffered before slow subscribers
    /// start missing events. Increase if you have slow event processing.
    pub fn event_channel_capacity(mut self, capacity: usize) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Set a session-level default timeout for all commands.
    ///
    /// **IMPORTANT:** Without a timeout, commands can hang indefinitely if the
    /// browser or connection fails. It is highly recommended to set a timeout
    /// (e.g., `Duration::from_secs(30)`).
    ///
    /// You can override this per-command using `send_command_with_timeout()`.
    pub fn command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = Some(timeout);
        self
    }

    /// Connect to the BiDi WebSocket with the configured options.
    ///
    /// This is a placeholder - actual implementation comes later.
    pub async fn connect(self, ws_url: &str) -> WebDriverResult<BiDiSession> {
        BiDiSession::connect_with_config(ws_url, self).await
    }
}

impl Default for BiDiSessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 5: Run tests**

```bash
cargo test --package thirtyfour --lib --features bidi extensions::bidi::tests
```

Expected: Tests pass but compilation will fail because `connect_with_config` doesn't exist yet - that's fine

**Step 6: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add BiDiSessionBuilder with tests

Add builder pattern for configuring BiDiSession before connection.
Supports event_channel_capacity and command_timeout configuration.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Add new fields to BiDiSession and update connect()

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Add imports**

Add to imports:

```rust
use std::sync::{atomic::AtomicBool, OnceLock};
```

**Step 2: Add new fields to BiDiSession**

Update `BiDiSession` struct (add after existing fields):

```rust
pub struct BiDiSession {
    // Existing fields...
    ws_sink: Arc<...>,
    command_id: Arc<AtomicU64>,
    pending: Arc<StdMutex<HashMap<...>>>,
    event_tx: broadcast::Sender<BiDiEvent>,

    // NEW: Connection state tracking
    connected: Arc<AtomicBool>,

    // NEW: Optional session-level command timeout
    command_timeout: Option<Duration>,

    // NEW: Typed event channels (lazy-initialized, wrapped in Arc for clone)
    network_tx: Arc<OnceLock<broadcast::Sender<NetworkEvent>>>,
    log_tx: Arc<OnceLock<broadcast::Sender<log::LogEvent>>>,
    browsing_context_tx: Arc<OnceLock<broadcast::Sender<BrowsingContextEvent>>>,
    script_tx: Arc<OnceLock<broadcast::Sender<ScriptEvent>>>,
}
```

**Step 3: Rename connect() and add connect_with_config()**

Replace the existing `connect()` method:

```rust
/// Connect to the BiDi WebSocket endpoint and spawn the dispatch task.
pub async fn connect(ws_url: &str) -> WebDriverResult<Self> {
    Self::connect_with_config(ws_url, BiDiSessionBuilder::new()).await
}

/// Connect to the BiDi WebSocket endpoint with custom configuration.
async fn connect_with_config(ws_url: &str, config: BiDiSessionBuilder) -> WebDriverResult<Self> {
    let (ws_stream, _) = connect_async(ws_url)
        .await
        .map_err(|e| WebDriverError::BiDi(format!("WebSocket connect failed: {e}")))?;

    let (sink, mut stream) = ws_stream.split();
    let (event_tx, _) = broadcast::channel::<BiDiEvent>(config.event_channel_capacity);
    let command_id = Arc::new(AtomicU64::new(1));
    let pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<WebDriverResult<Value>>>>> =
        Arc::new(StdMutex::new(HashMap::new()));
    let connected = Arc::new(AtomicBool::new(true));

    // Typed event channels
    let network_tx = Arc::new(OnceLock::new());
    let log_tx = Arc::new(OnceLock::new());
    let browsing_context_tx = Arc::new(OnceLock::new());
    let script_tx = Arc::new(OnceLock::new());

    let pending_clone = Arc::clone(&pending);
    let event_tx_clone = event_tx.clone();
    let connected_clone = Arc::clone(&connected);
    let network_tx_clone = Arc::clone(&network_tx);
    let log_tx_clone = Arc::clone(&log_tx);
    let browsing_context_tx_clone = Arc::clone(&browsing_context_tx);
    let script_tx_clone = Arc::clone(&script_tx);

    tracing::debug!(url = %ws_url, "BiDi WebSocket connected");

    // Background dispatch task
    let span = tracing::debug_span!("bidi_dispatch", url = %ws_url);
    tokio::spawn(
        async move {
            while let Some(msg) = stream.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t,
                    Ok(Message::Close(_)) => {
                        connected_clone.store(false, std::sync::atomic::Ordering::Relaxed);
                        let _ = event_tx_clone.send(BiDiEvent::ConnectionClosed);
                        tracing::debug!("BiDi WebSocket connection closed");
                        break;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        tracing::error!("BiDi WebSocket error: {e}");
                        connected_clone.store(false, std::sync::atomic::Ordering::Relaxed);
                        let _ = event_tx_clone.send(BiDiEvent::ConnectionClosed);
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
                                match pending_clone.lock() {
                                    Ok(mut map) => map.remove(&id),
                                    Err(poisoned) => {
                                        tracing::error!("BiDi pending commands mutex poisoned");
                                        poisoned.into_inner().remove(&id)
                                    }
                                }
                            };
                            if let Some(tx) = sender {
                                let is_success =
                                    v.get("type").and_then(Value::as_str) == Some("success");
                                let result = if is_success {
                                    Ok(v.get("result").cloned().unwrap_or(Value::Null))
                                } else {
                                    let msg = v
                                        .get("message")
                                        .and_then(Value::as_str)
                                        .unwrap_or("unknown BiDi error")
                                        .to_string();
                                    Err(WebDriverError::BiDi(msg))
                                };
                                tracing::trace!(id = %id, success = %is_success, "Received BiDi response");
                                let _ = tx.send(result);
                            }
                        }
                    }
                    Some("event") => {
                        let method = v
                            .get("method")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let params = v.get("params").cloned().unwrap_or(Value::Null);
                        tracing::trace!(method = %method, "Received BiDi event");
                        let event = parse_event(&method, params);

                        // Broadcast to typed channels if initialized
                        match &event {
                            BiDiEvent::Network(e) => {
                                if let Some(tx) = network_tx_clone.get() {
                                    let _ = tx.send(e.clone());
                                }
                            }
                            BiDiEvent::Log(e) => {
                                if let Some(tx) = log_tx_clone.get() {
                                    let _ = tx.send(e.clone());
                                }
                            }
                            BiDiEvent::BrowsingContext(e) => {
                                if let Some(tx) = browsing_context_tx_clone.get() {
                                    let _ = tx.send(e.clone());
                                }
                            }
                            BiDiEvent::Script(e) => {
                                if let Some(tx) = script_tx_clone.get() {
                                    let _ = tx.send(e.clone());
                                }
                            }
                            _ => {}
                        }

                        // Broadcast to unified channel
                        let _ = event_tx_clone.send(event);
                    }
                    _ => {}
                }
            }
        }
        .instrument(span),
    );

    Ok(Self {
        ws_sink: Arc::new(TokioMutex::new(sink)),
        command_id,
        pending,
        event_tx,
        connected,
        command_timeout: config.command_timeout,
        network_tx,
        log_tx,
        browsing_context_tx,
        script_tx,
    })
}
```

**Step 4: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi
```

Expected: PASS

**Step 5: Run tests**

```bash
cargo test --package thirtyfour --lib --features bidi
```

Expected: All tests PASS

**Step 6: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add connection state and typed channel fields

Add new fields to BiDiSession:
- connected: Arc<AtomicBool> for connection state tracking
- command_timeout: Option<Duration> for session-level timeouts
- network_tx, log_tx, browsing_context_tx, script_tx: Arc<OnceLock> channels

Update connect() to use BiDiSessionBuilder configuration.
Add dual-broadcast: events sent to both unified and typed channels.
Add structured tracing for connection lifecycle and events.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Add is_connected() and timeout methods

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Add is_connected() method**

After `subscribe_events()` method, add:

```rust
/// Check if the WebSocket connection is still alive.
///
/// Returns `false` if the connection has closed. Note that this is a
/// best-effort check - the connection could close immediately after
/// this returns `true`.
///
/// For reliable connection state change detection, subscribe to
/// `BiDiEvent::ConnectionClosed` events.
pub fn is_connected(&self) -> bool {
    self.connected.load(std::sync::atomic::Ordering::Relaxed)
}
```

**Step 2: Add send_command_with_timeout() method**

After the existing `send_command()` method, add:

```rust
/// Send a BiDi command with an explicit timeout override.
///
/// Use this when a specific command needs a different timeout than the
/// session default configured via [`BiDiSessionBuilder::command_timeout`].
pub async fn send_command_with_timeout(
    &self,
    method: &str,
    params: Value,
    timeout: Duration,
) -> WebDriverResult<Value> {
    let id = self.command_id.fetch_add(1, Ordering::SeqCst);
    let msg = json!({ "id": id, "method": method, "params": params });
    let text = serde_json::to_string(&msg)
        .map_err(|e| WebDriverError::BiDi(format!("serialise error: {e}")))?;

    let (tx, rx) = oneshot::channel();
    {
        match self.pending.lock() {
            Ok(mut map) => {
                map.insert(id, tx);
            }
            Err(poisoned) => {
                tracing::error!("BiDi pending commands mutex poisoned");
                poisoned.into_inner().insert(id, tx);
            }
        }
    }

    tracing::trace!(
        method = %method,
        id = %id,
        timeout = ?timeout,
        "Sending BiDi command with timeout"
    );

    self.ws_sink
        .lock()
        .await
        .send(Message::Text(text))
        .await
        .map_err(|e| WebDriverError::BiDi(format!("WebSocket send failed: {e}")))?;

    tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_| {
            WebDriverError::BiDi(format!("command '{}' timed out after {:?}", method, timeout))
        })?
        .map_err(|_| WebDriverError::BiDi("response channel closed".to_string()))?
}
```

**Step 3: Update send_command() to support session timeout**

Replace existing `send_command()`:

```rust
/// Send a BiDi command and await the response.
///
/// If a session-level timeout was configured via [`BiDiSessionBuilder::command_timeout`],
/// the command will timeout after that duration. Otherwise, it waits indefinitely.
///
/// **IMPORTANT:** Without a timeout, commands can hang indefinitely if the browser
/// or connection fails. Use [`BiDiSessionBuilder::command_timeout`] to set a
/// session-level default, or use [`Self::send_command_with_timeout`] for per-command
/// timeouts.
pub async fn send_command(&self, method: &str, params: Value) -> WebDriverResult<Value> {
    match self.command_timeout {
        Some(timeout) => self.send_command_with_timeout(method, params, timeout).await,
        None => {
            let id = self.command_id.fetch_add(1, Ordering::SeqCst);
            let msg = json!({ "id": id, "method": method, "params": params });
            let text = serde_json::to_string(&msg)
                .map_err(|e| WebDriverError::BiDi(format!("serialise error: {e}")))?;

            let (tx, rx) = oneshot::channel();
            {
                match self.pending.lock() {
                    Ok(mut map) => {
                        map.insert(id, tx);
                    }
                    Err(poisoned) => {
                        tracing::error!("BiDi pending commands mutex poisoned");
                        poisoned.into_inner().insert(id, tx);
                    }
                }
            }

            tracing::trace!(
                method = %method,
                id = %id,
                "Sending BiDi command"
            );

            self.ws_sink
                .lock()
                .await
                .send(Message::Text(text))
                .await
                .map_err(|e| WebDriverError::BiDi(format!("WebSocket send failed: {e}")))?;

            rx.await
                .map_err(|_| WebDriverError::BiDi("response channel closed".to_string()))?
        }
    }
}
```

**Step 4: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi
```

Expected: PASS

**Step 5: Run tests**

```bash
cargo test --package thirtyfour --lib --features bidi
```

Expected: PASS

**Step 6: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add is_connected() and timeout support

Add methods:
- is_connected(): Check WebSocket connection state
- send_command_with_timeout(): Per-command timeout override
- Update send_command() to respect session-level timeout

Add tracing for command send with timeout info.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Add typed event channel methods (manual pattern)

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Add typed event receiver methods**

After `subscribe_events()` and before `is_connected()`, add:

```rust
/// Get a typed receiver for network events.
///
/// **Note:** This method does NOT automatically subscribe to network events.
/// You must first call `bidi.session().subscribe(&["network.*"], &[]).await?`
/// to tell the browser to start sending network events.
///
/// For automatic subscription, use [`Self::subscribe_network`] instead.
pub fn network_events(&self) -> broadcast::Receiver<NetworkEvent> {
    self.network_tx
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(256);
            tx
        })
        .subscribe()
}

/// Get a typed receiver for log events.
///
/// **Note:** This method does NOT automatically subscribe to log events.
/// You must first call `bidi.session().subscribe(&["log.*"], &[]).await?`
/// to tell the browser to start sending log events.
///
/// For automatic subscription, use [`Self::subscribe_log`] instead.
pub fn log_events(&self) -> broadcast::Receiver<log::LogEvent> {
    self.log_tx
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(256);
            tx
        })
        .subscribe()
}

/// Get a typed receiver for browsing context events.
///
/// **Note:** This method does NOT automatically subscribe to browsing context events.
/// You must first call `bidi.session().subscribe(&["browsingContext.*"], &[]).await?`
/// to tell the browser to start sending browsing context events.
///
/// For automatic subscription, use [`Self::subscribe_browsing_context`] instead.
pub fn browsing_context_events(&self) -> broadcast::Receiver<BrowsingContextEvent> {
    self.browsing_context_tx
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(256);
            tx
        })
        .subscribe()
}

/// Get a typed receiver for script events.
///
/// **Note:** This method does NOT automatically subscribe to script events.
/// You must first call `bidi.session().subscribe(&["script.*"], &[]).await?`
/// to tell the browser to start sending script events.
///
/// For automatic subscription, use [`Self::subscribe_script`] instead.
pub fn script_events(&self) -> broadcast::Receiver<ScriptEvent> {
    self.script_tx
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(256);
            tx
        })
        .subscribe()
}
```

**Step 2: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi
```

Expected: PASS

**Step 3: Run tests**

```bash
cargo test --package thirtyfour --lib --features bidi
```

Expected: PASS

**Step 4: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add typed event channel receivers (manual pattern)

Add methods to get typed receivers:
- network_events(): Receiver<NetworkEvent>
- log_events(): Receiver<log::LogEvent>
- browsing_context_events(): Receiver<BrowsingContextEvent>
- script_events(): Receiver<ScriptEvent>

These methods do NOT auto-subscribe - users must call session.subscribe() first.
Channels are lazy-initialized with OnceLock.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Add auto-subscribe convenience methods

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Add auto-subscribe methods**

After the typed event receiver methods, add:

```rust
/// Subscribe to network events and return a typed receiver.
///
/// Automatically calls `session.subscribe(&["network.*"], &[])` to tell
/// the browser to start sending network events.
///
/// For manual control, use [`Self::network_events`] instead.
pub async fn subscribe_network(&self) -> WebDriverResult<broadcast::Receiver<NetworkEvent>> {
    self.session()
        .subscribe(&["network.*"], &[])
        .await?;
    Ok(self.network_events())
}

/// Subscribe to log events and return a typed receiver.
///
/// Automatically calls `session.subscribe(&["log.*"], &[])` to tell
/// the browser to start sending log events.
///
/// For manual control, use [`Self::log_events`] instead.
pub async fn subscribe_log(&self) -> WebDriverResult<broadcast::Receiver<log::LogEvent>> {
    self.session()
        .subscribe(&["log.*"], &[])
        .await?;
    Ok(self.log_events())
}

/// Subscribe to browsing context events and return a typed receiver.
///
/// Automatically calls `session.subscribe(&["browsingContext.*"], &[])` to tell
/// the browser to start sending browsing context events.
///
/// For manual control, use [`Self::browsing_context_events`] instead.
pub async fn subscribe_browsing_context(
    &self,
) -> WebDriverResult<broadcast::Receiver<BrowsingContextEvent>> {
    self.session()
        .subscribe(&["browsingContext.*"], &[])
        .await?;
    Ok(self.browsing_context_events())
}

/// Subscribe to script events and return a typed receiver.
///
/// Automatically calls `session.subscribe(&["script.*"], &[])` to tell
/// the browser to start sending script events.
///
/// For manual control, use [`Self::script_events`] instead.
pub async fn subscribe_script(&self) -> WebDriverResult<broadcast::Receiver<ScriptEvent>> {
    self.session()
        .subscribe(&["script.*"], &[])
        .await?;
    Ok(self.script_events())
}
```

**Step 2: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi
```

Expected: PASS

**Step 3: Run tests**

```bash
cargo test --package thirtyfour --lib --features bidi
```

Expected: PASS

**Step 4: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add auto-subscribe convenience methods

Add methods that auto-subscribe and return typed receivers:
- subscribe_network(): auto-subscribes to network.* events
- subscribe_log(): auto-subscribes to log.* events
- subscribe_browsing_context(): auto-subscribes to browsingContext.* events
- subscribe_script(): auto-subscribes to script.* events

These methods provide one-call convenience for common use cases.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Add bidi_connect_builder() to WebDriver

**Files:**
- Modify: `thirtyfour/thirtyfour/src/web_driver.rs`

**Step 1: Add bidi_connect_builder() method**

Find the existing `bidi_connect()` method and add this after it:

```rust
#[cfg(feature = "bidi")]
/// Get a builder for custom BiDi connection configuration.
///
/// Use this to configure options like command timeout and event channel
/// capacity before connecting.
///
/// # Examples
///
/// ```no_run
/// # use std::time::Duration;
/// # use thirtyfour::prelude::*;
/// # async fn example(driver: &WebDriver) -> WebDriverResult<()> {
/// let bidi = driver
///     .bidi_connect_builder()
///     .command_timeout(Duration::from_secs(30))
///     .event_channel_capacity(512)
///     .connect()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub fn bidi_connect_builder(&self) -> crate::extensions::bidi::BiDiSessionBuilder {
    crate::extensions::bidi::BiDiSessionBuilder::new()
}
```

Wait, we need to pass the ws_url to the builder's connect method. Let me revise:

Actually, looking at the builder design, the connect() method takes ws_url as a parameter, so the builder doesn't need the WebDriver reference. But we need a way to get the ws_url from WebDriver.

Let me revise the approach to make it ergonomic:

**Step 1 (revised): Add builder method that returns configured builder with ws_url**

We have two options:
A) Return a builder that still needs `.connect(ws_url)` called
B) Add a method to builder that accepts WebDriver and extracts ws_url

Let's go with option B for better ergonomics:

Update BiDiSessionBuilder in mod.rs to add:

```rust
/// Connect using WebDriver's session webSocketUrl.
///
/// This is a convenience method that extracts the webSocketUrl from the
/// WebDriver session and connects using the builder configuration.
pub async fn connect_from_driver(
    self,
    driver: &crate::WebDriver,
) -> WebDriverResult<BiDiSession> {
    let ws_url = driver
        .handle
        .websocket_url
        .as_deref()
        .ok_or_else(|| {
            crate::error::WebDriverError::BiDi(
                "No webSocketUrl in session capabilities. \
                 Enable BiDi in your browser capabilities \
                 (e.g., for Chrome: set 'webSocketUrl: true')."
                    .to_string(),
            )
        })?;
    self.connect(ws_url).await
}
```

Then in web_driver.rs:

```rust
#[cfg(feature = "bidi")]
/// Get a builder for custom BiDi connection configuration.
///
/// # Examples
///
/// ```no_run
/// # use std::time::Duration;
/// # use thirtyfour::prelude::*;
/// # async fn example(driver: &WebDriver) -> WebDriverResult<()> {
/// let bidi = driver
///     .bidi_connect_builder()
///     .command_timeout(Duration::from_secs(30))
///     .event_channel_capacity(512)
///     .connect_from_driver(&driver)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub fn bidi_connect_builder(&self) -> crate::extensions::bidi::BiDiSessionBuilder {
    crate::extensions::bidi::BiDiSessionBuilder::new()
}
```

This is getting awkward. Let me use a simpler approach - just make the builder accept the driver in its build step:

Actually, the cleanest is to keep it simple: builder returns a builder, user calls either `.connect(ws_url)` or they use the convenience method on WebDriver. Let me just add the simple method:

**Step 1 (final, simple approach)**:

```rust
#[cfg(feature = "bidi")]
/// Get a builder for custom BiDi connection configuration.
///
/// The builder's `connect()` method requires the WebSocket URL.
/// You can get this from `driver.handle.websocket_url`.
///
/// # Examples
///
/// ```no_run
/// # use std::time::Duration;
/// # use thirtyfour::prelude::*;
/// # async fn example(driver: &WebDriver) -> WebDriverResult<()> {
/// let ws_url = driver.handle.websocket_url.as_deref()
///     .ok_or_else(|| WebDriverError::BiDi("No webSocketUrl".into()))?;
///
/// let bidi = BiDiSessionBuilder::new()
///     .command_timeout(Duration::from_secs(30))
///     .event_channel_capacity(512)
///     .connect(ws_url)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub fn bidi_connect_builder(&self) -> crate::extensions::bidi::BiDiSessionBuilder {
    crate::extensions::bidi::BiDiSessionBuilder::new()
}
```

Actually wait - I should make BiDiSessionBuilder::connect() able to work without WebDriver. Looking back at the design, the builder should be independent. So a better approach is:

Users can either:
1. Use `driver.bidi_connect()` for simple case (existing)
2. Use `BiDiSessionBuilder::new()...connect(ws_url)` directly

And we add a helper on WebDriver for getting ws_url. But that's extra work. Let's just export BiDiSessionBuilder and document how to use it:

**Step 1 (simplest)**:

In lib.rs, update the bidi exports:

```rust
#[cfg(feature = "bidi")]
pub use extensions::bidi::{BiDiEvent, BiDiSession, BiDiSessionBuilder};
```

Then users can use it directly:

```rust
use thirtyfour::BiDiSessionBuilder;

let ws_url = driver.handle.websocket_url.as_deref().unwrap();
let bidi = BiDiSessionBuilder::new()
    .command_timeout(Duration::from_secs(30))
    .connect(ws_url)
    .await?;
```

But that's not as ergonomic. Let me add the helper method on WebDriver that makes it cleaner:

**Step 1 (ergonomic approach)**:

Add to web_driver.rs after bidi_connect():

```rust
#[cfg(feature = "bidi")]
/// Connect to BiDi using a builder for custom configuration.
///
/// # Examples
///
/// ```no_run
/// # use std::time::Duration;
/// # use thirtyfour::prelude::*;
/// # async fn example(driver: &WebDriver) -> WebDriverResult<()> {
/// use thirtyfour::BiDiSessionBuilder;
///
/// let bidi = BiDiSessionBuilder::new()
///     .command_timeout(Duration::from_secs(30))
///     .event_channel_capacity(512)
///     .connect_with_driver(driver)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub async fn bidi_connect_with_builder(
    &self,
    builder: crate::extensions::bidi::BiDiSessionBuilder,
) -> crate::error::WebDriverResult<crate::extensions::bidi::BiDiSession> {
    let ws_url = self.handle.websocket_url.as_deref().ok_or_else(|| {
        crate::prelude::WebDriverError::BiDi(
            "No webSocketUrl in session capabilities. \
             Enable BiDi in your browser capabilities \
             (e.g., for Chrome: set 'webSocketUrl: true')."
                .to_string(),
        )
    })?;
    builder.connect(ws_url).await
}
```

This way users can:
```rust
let bidi = driver
    .bidi_connect_with_builder(
        BiDiSessionBuilder::new()
            .command_timeout(Duration::from_secs(30))
    )
    .await?;
```

**Step 2: Update lib.rs exports**

```rust
#[cfg(feature = "bidi")]
pub use extensions::bidi::{BiDiEvent, BiDiSession, BiDiSessionBuilder};
```

**Step 3: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi
```

Expected: PASS

**Step 4: Run tests**

```bash
cargo test --package thirtyfour --lib --features bidi
```

Expected: PASS

**Step 5: Commit**

```bash
git add thirtyfour/thirtyfour/src/web_driver.rs thirtyfour/thirtyfour/src/lib.rs
git commit -m "feat(bidi): add bidi_connect_with_builder() to WebDriver

Add WebDriver::bidi_connect_with_builder() for custom BiDi configuration.
Export BiDiSessionBuilder in lib.rs for public use.

Users can now configure timeouts and event channel capacity:
  driver.bidi_connect_with_builder(
      BiDiSessionBuilder::new().command_timeout(Duration::from_secs(30))
  ).await

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 7: Add module-level documentation

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Add comprehensive module docs**

Replace the existing module doc comment at the top:

```rust
//! WebDriver BiDi bidirectional protocol support.
//!
//! Enable with the `bidi` cargo feature.
//!
//! ## Command Timeouts
//!
//! **IMPORTANT:** By default, BiDi commands have NO timeout and can hang indefinitely
//! if the browser or connection fails. It is **highly recommended** to configure a timeout:
//!
//! ```no_run
//! use std::time::Duration;
//! use thirtyfour::prelude::*;
//! use thirtyfour::BiDiSessionBuilder;
//!
//! # async fn example(driver: WebDriver) -> WebDriverResult<()> {
//! let bidi = BiDiSessionBuilder::new()
//!     .command_timeout(Duration::from_secs(30))
//!     .connect_with_driver(&driver)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! For commands that need different timeouts (e.g., slow script evaluation), use
//! `send_command_with_timeout()`:
//!
//! ```no_run
//! # use std::time::Duration;
//! # use thirtyfour::prelude::*;
//! # async fn example(bidi: &BiDiSession) -> WebDriverResult<()> {
//! let result = bidi
//!     .send_command_with_timeout(
//!         "script.evaluate",
//!         serde_json::json!({"expression": "longRunningFunction()"}),
//!         Duration::from_secs(60), // Override session default
//!     )
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Event Subscription Patterns
//!
//! BiDiSession provides three ways to subscribe to events, giving you flexibility
//! based on your needs:
//!
//! ### Pattern 1: Unified channel (all events)
//!
//! Receive all event types in one stream. Good for handling multiple domains.
//!
//! ```no_run
//! # use thirtyfour::prelude::*;
//! # use thirtyfour::extensions::bidi::BiDiEvent;
//! # async fn example(bidi: &BiDiSession) -> WebDriverResult<()> {
//! let mut rx = bidi.subscribe_events();
//! while let Ok(event) = rx.recv().await {
//!     match event {
//!         BiDiEvent::Network(e) => println!("Network: {:?}", e),
//!         BiDiEvent::Log(e) => println!("Log: {:?}", e),
//!         BiDiEvent::ConnectionClosed => break,
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Pattern 2: Typed channel (manual subscription)
//!
//! Get type-safe event receivers with explicit subscription control.
//!
//! ```no_run
//! # use thirtyfour::prelude::*;
//! # async fn example(bidi: &BiDiSession) -> WebDriverResult<()> {
//! // Explicitly subscribe to network events
//! bidi.session().subscribe(&["network.*"], &[]).await?;
//!
//! // Get typed receiver (no auto-subscribe)
//! let mut rx = bidi.network_events();
//! while let Ok(event) = rx.recv().await {
//!     // event is NetworkEvent - compile-time type safety
//!     println!("Network event: {:?}", event);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Pattern 3: Auto-subscribe (convenience)
//!
//! One call subscribes and returns a typed receiver.
//!
//! ```no_run
//! # use thirtyfour::prelude::*;
//! # async fn example(bidi: &BiDiSession) -> WebDriverResult<()> {
//! // Automatically subscribes to network.* events
//! let mut rx = bidi.subscribe_network().await?;
//!
//! while let Ok(event) = rx.recv().await {
//!     println!("Network event: {:?}", event);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! All three patterns can be used simultaneously - events are broadcast to all
//! active channels.
```

**Step 2: Verify compilation**

```bash
cargo check --package thirtyfour --features bidi
```

Expected: PASS

**Step 3: Commit**

```bash
git add thirtyfour/thirtyfour/src/extensions/bidi/mod.rs
git commit -m "docs(bidi): add comprehensive module documentation

Add module-level docs explaining:
- Command timeout importance and configuration
- Three event subscription patterns with examples
- Usage examples for common scenarios

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 8: Create integration test documentation

**Files:**
- Create: `thirtyfour/tests/bidi_integration_tests.md`

**Step 1: Create documentation file**

```markdown
# BiDi Integration Tests

## Requirements

- Chrome 115+ or Firefox 119+ with BiDi support
- Running WebDriver server (chromedriver/geckodriver)

## Browser Setup

### Chrome with Chromedriver

\`\`\`bash
chromedriver --port=9515
\`\`\`

### Firefox with Geckodriver

\`\`\`bash
geckodriver --port=4444
\`\`\`

## Running Tests

\`\`\`bash
# Run all BiDi integration tests
cargo test --test bidi --features bidi

# Run specific test
cargo test --test bidi --features bidi test_network_intercept

# Run with trace logging
RUST_LOG=thirtyfour::extensions::bidi=trace cargo test --test bidi --features bidi
\`\`\`

## Writing Integration Tests

Integration tests should:
- Use `#[tokio::test]` for async tests
- Set `webSocketUrl: true` in capabilities for Chrome
- Handle cleanup in case of test failure
- Test one feature per test function
- Document browser-specific behaviors

Example test structure:

\`\`\`rust
#[tokio::test]
async fn test_network_intercept() -> WebDriverResult<()> {
    let mut caps = DesiredCapabilities::chrome();
    caps.insert_base_capability("webSocketUrl".into(), true.into());

    let driver = WebDriver::new("http://localhost:9515", caps).await?;
    let bidi = BiDiSessionBuilder::new()
        .command_timeout(Duration::from_secs(10))
        .connect_with_driver(&driver)
        .await?;

    // Test network interception
    let mut rx = bidi.subscribe_network().await?;
    driver.goto("https://example.com").await?;

    // Wait for network event
    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("channel closed");

    println!("Received event: {:?}", event);

    driver.quit().await?;
    Ok(())
}
\`\`\`

## Common Issues

### Chrome: webSocketUrl not returned

Ensure you set the capability:

\`\`\`rust
caps.insert_base_capability("webSocketUrl".into(), true.into());
\`\`\`

### Events not received

Make sure you call `session.subscribe()` before expecting events:

\`\`\`rust
bidi.session().subscribe(&["network.*"], &[]).await?;
\`\`\`

Or use the auto-subscribe convenience methods:

\`\`\`rust
let rx = bidi.subscribe_network().await?;
\`\`\`

### Timeout errors

Consider increasing the command timeout for slow operations:

\`\`\`rust
let bidi = BiDiSessionBuilder::new()
    .command_timeout(Duration::from_secs(30))
    .connect_with_driver(&driver)
    .await?;
\`\`\`
```

**Step 2: Commit**

```bash
git add thirtyfour/tests/bidi_integration_tests.md
git commit -m "docs(bidi): add integration test documentation

Create comprehensive guide for running and writing BiDi integration tests.
Includes browser setup, test examples, and troubleshooting.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 9: Update README.md

**Files:**
- Modify: `thirtyfour/README.md`

**Step 1: Find and update feature flags section**

Locate the "Feature Flags" or "Features" section in README.md and add:

```markdown
- `bidi`: Enable WebDriver BiDi (bidirectional protocol) support for real-time
  event handling, network interception, script evaluation, and more.
  Requires Chrome 115+ or Firefox 119+. See `examples/bidi_network_intercept.rs`
  for usage examples.
```

If there is no Features section, add one before the License section.

**Step 2: Verify the file looks correct**

```bash
cat README.md | grep -A 5 -B 5 bidi
```

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update README with bidi feature flag documentation

Add bidi feature flag to README feature list with description,
requirements, and example reference.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 10: Final verification

**Files:**
- None (verification only)

**Step 1: Run full test suite**

```bash
cargo test --package thirtyfour --lib
```

Expected: All tests PASS (without bidi feature)

**Step 2: Run with bidi feature**

```bash
cargo test --package thirtyfour --lib --features bidi
```

Expected: All tests PASS

**Step 3: Run clippy**

```bash
cargo clippy --package thirtyfour --features bidi -- -D warnings
```

Expected: PASS (zero warnings)

**Step 4: Check all features**

```bash
cargo check --package thirtyfour --all-features
```

Expected: PASS

**Step 5: Verify example compiles**

```bash
cargo check --package thirtyfour --features bidi --example bidi_network_intercept
```

Expected: PASS

**Step 6: Review git log**

```bash
git log --oneline -15
```

Expected: See all commits from this plan in logical order

---

## Success Criteria

All of these must be true:

1. ✅ BiDiSessionBuilder exists and has tests
2. ✅ BiDiSession has new fields (connected, command_timeout, typed channels)
3. ✅ is_connected() method works
4. ✅ send_command_with_timeout() method exists
5. ✅ send_command() respects session-level timeout
6. ✅ Typed event receivers work (network_events, etc.)
7. ✅ Auto-subscribe convenience methods work
8. ✅ bidi_connect_with_builder() on WebDriver
9. ✅ Comprehensive module documentation
10. ✅ Integration test documentation created
11. ✅ README updated with bidi feature
12. ✅ All tests pass with and without bidi feature
13. ✅ Clippy passes with -D warnings
14. ✅ Zero breaking changes to existing API

---

## Notes

- This plan implements the dual-channel architecture from the design doc
- All existing BiDi functionality remains unchanged
- The builder pattern is opt-in - existing code continues to work
- Typed channels are lazy-initialized only when used
- Connection state tracking uses AtomicBool for lock-free reads
- Tracing is added at appropriate levels (debug/trace)
- Documentation emphasizes timeout importance
