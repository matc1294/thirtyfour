# WebDriver BiDi Implementation Design

**Date:** 2026-02-19
**Crate:** `thirtyfour` v0.36.1+
**Reference:** [Selenium Python BiDi](https://github.com/SeleniumHQ/selenium/tree/trunk/py/selenium/webdriver/common/bidi)

---

## Goals

Add full WebDriver BiDi (bidirectional) protocol support to `thirtyfour` as a fresh, Rust-idiomatic implementation. BiDi replaces the unidirectional HTTP-only CDP extension with a persistent WebSocket channel, enabling real-time event subscriptions and request interception.

**Also:** Remove the `selenium-manager` dependency and feature from `Cargo.toml`.

---

## Non-Goals

- Reuse or port the existing `vk/5201-principles` branch — this is a clean implementation.
- Replace existing CDP extension — it stays as-is for backwards compatibility.
- Change the existing HTTP-based WebDriver command path.

---

## Dependencies

### Remove
- `selenium-manager` dependency and feature from `thirtyfour/Cargo.toml`

### Add (feature-gated)
```toml
[features]
default = ["reqwest", "rustls-tls", "component"]
bidi = ["dep:tokio-tungstenite"]

[dependencies]
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"], optional = true }
```

`futures-util` is already a dependency and will be reused.

---

## Feature Flag

All BiDi code lives behind `#[cfg(feature = "bidi")]`. Users opt in with:

```toml
thirtyfour = { version = "...", features = ["bidi"] }
```

---

## Architecture

### Core: `BiDiSession`

```
WebDriver
  └── .bidi_connect().await → BiDiSession
         ├── ws_sink: Mutex<SplitSink<WsStream, Message>>
         │       ← write commands to WebSocket
         ├── command_id: AtomicU64
         │       ← auto-incrementing JSON-RPC IDs
         ├── pending: DashMap<u64, oneshot::Sender<Value>>
         │       ← in-flight commands awaiting responses
         └── event_tx: broadcast::Sender<BiDiEvent>
                 ← fan-out to all domain event subscribers
```

A **background tokio task** is spawned on `bidi_connect()`:
- Reads incoming WebSocket frames continuously
- Parses each JSON message:
  - If `"type": "success"` or `"type": "error"` and has `"id"` → routes to matching `oneshot::Sender` in `pending`
  - If `"type": "event"` → deserializes into `BiDiEvent` and broadcasts on `event_tx`

### Command Round-Trip

```
domain.some_command(params).await
  → BiDiSession::send("domain.command", json!({...}))
  → id = command_id.fetch_add(1)
  → insert oneshot::channel into pending[id]
  → serialize {"id": id, "method": "...", "params": {...}}
  → send over WebSocket
  → await oneshot receiver
  → deserialize result Value
  → return typed response struct
```

### Event Fan-Out

```
BiDiSession background task
  → receives raw event JSON
  → parses into BiDiEvent enum variant
  → event_tx.send(event)          ← broadcast to all subscribers

domain.subscribe()
  → returns event_tx.subscribe()  ← broadcast::Receiver<BiDiEvent>
  → caller filters to relevant variants in their .await loop
```

### `BiDiEvent` Enum

```rust
pub enum BiDiEvent {
    Network(NetworkEvent),
    Log(LogEvent),
    Script(ScriptEvent),
    BrowsingContext(BrowsingContextEvent),
    Browser(BrowserEvent),
    Console(ConsoleEvent),
    // ... one variant per domain that emits events
}
```

### Domain Accessor Pattern

`BiDiSession` exposes lightweight domain accessors that borrow `self`:

```rust
impl BiDiSession {
    pub fn network(&self) -> Network<'_>
    pub fn log(&self) -> Log<'_>
    pub fn script(&self) -> Script<'_>
    pub fn browser(&self) -> Browser<'_>
    pub fn browsing_context(&self) -> BrowsingContext<'_>
    pub fn console(&self) -> Console<'_>
    pub fn input(&self) -> Input<'_>
    pub fn permissions(&self) -> Permissions<'_>
    pub fn storage(&self) -> Storage<'_>
    pub fn webextension(&self) -> WebExtension<'_>
    pub fn emulation(&self) -> Emulation<'_>
    pub fn cdp(&self) -> Cdp<'_>
    pub fn session(&self) -> Session<'_>
}
```

Each domain struct holds `session: &'a BiDiSession`.

---

## Module Layout

```
thirtyfour/src/extensions/bidi/
├── mod.rs              ← BiDiSession, BiDiEvent, connect logic,
│                          send_command(), background dispatch task
├── session.rs          ← subscribe(events, contexts), unsubscribe
├── network.rs          ← add_intercept, remove_intercept,
│                          continue_request, continue_response,
│                          fail_request, provide_response,
│                          NetworkEvent enum
├── log.rs              ← subscribe → Receiver<LogEntry>
├── script.rs           ← addPreloadScript, removePreloadScript,
│                          evaluate, callFunction, disown, RealmEvent
├── browser.rs          ← close, createUserContext, removeUserContext,
│                          getUserContexts
├── browsing_context.rs ← activate, captureScreenshot, close, create,
│                          navigate, reload, traverseHistory, setViewport,
│                          NavigationEvent
├── console.rs          ← thin wrapper over log events filtering console.*
├── input.rs            ← perform (BiDi Actions), release
├── permissions.rs      ← setPermission, PermissionState enum
├── storage.rs          ← getCookies, setCookie, deleteCookies
├── webextension.rs     ← install, uninstall
├── emulation.rs        ← setGeolocationOverride, setDevicePosture
└── cdp.rs              ← sendCommand, resolveRealm (BiDi CDP passthrough)
```

### Integration point

```rust
// thirtyfour/src/extensions/mod.rs
#[cfg(feature = "bidi")]
pub mod bidi;

// thirtyfour/src/web_driver.rs  (feature-gated)
#[cfg(feature = "bidi")]
pub async fn bidi_connect(&self) -> WebDriverResult<bidi::BiDiSession> {
    let ws_url = /* read webSocketUrl from session capabilities */;
    bidi::BiDiSession::connect(ws_url).await
}
```

---

## Wire Protocol

WebDriver BiDi uses JSON-RPC-like framing over WebSocket:

```json
// Command (client → server)
{"id": 1, "method": "network.addIntercept", "params": {"phases": ["beforeRequestSent"]}}

// Success response (server → client)
{"id": 1, "type": "success", "result": {"intercept": "abc123"}}

// Error response (server → client)
{"id": 1, "type": "error", "error": "unknown command", "message": "..."}

// Event (server → client, unsolicited)
{"type": "event", "method": "network.beforeRequestSent", "params": {...}}
```

---

## Error Handling

Add a new variant to the existing `WebDriverError` enum:

```rust
#[error("BiDi error: {0}")]
BiDi(String),
```

No separate error type — stays consistent with the rest of the library. BiDi protocol errors (type=`"error"` responses) map to `WebDriverError::BiDi`.

---

## Usage Example

```rust
// Connect BiDi channel
let bidi = driver.bidi_connect().await?;

// Subscribe to session-level events
bidi.session().subscribe(&["network.beforeRequestSent"]).await?;

// Add network intercept
let network = bidi.network();
let intercept_id = network.add_intercept(&[InterceptPhase::BeforeRequestSent]).await?;

// Listen to events
let mut rx = network.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        if let BiDiEvent::Network(NetworkEvent::BeforeRequestSent(e)) = event {
            println!("Request: {} {}", e.request.method, e.request.url);
        }
    }
});

// Navigate
driver.goto("https://example.com").await?;

// Clean up
network.remove_intercept(intercept_id).await?;
```

---

## Testing Strategy

- Unit tests within each domain module using mock `send_command` responses
- Integration tests (behind `#[ignore]`) that require a live BiDi-capable driver (Chrome 115+, Firefox 119+)
- One example file: `examples/bidi_network_intercept.rs`
