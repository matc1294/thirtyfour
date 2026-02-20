# WebDriver BiDi Enhancements Design

**Date:** 2026-02-19
**Crate:** `thirtyfour` v0.36.1+
**Based on:** Code review suggestions from initial BiDi implementation

---

## Goals

Enhance the existing WebDriver BiDi implementation with ergonomic improvements and flexible configuration options based on code review feedback:

1. **Typed event channels** - Provide domain-specific event receivers for type safety
2. **Flexible subscription patterns** - Give users three ways to subscribe to events
3. **Configurable timeouts** - Allow session-level and per-command timeout configuration
4. **Connection state tracking** - Add `is_connected()` method for monitoring
5. **Enhanced observability** - Add structured tracing throughout BiDi operations
6. **Builder pattern** - Enable configuration before connection
7. **Better documentation** - Document timeout recommendations and subscription patterns

---

## Non-Goals

- Replacing the existing unified `BiDiEvent` channel (it stays)
- Breaking changes to the existing API
- Adding default timeouts (library principle: explicit over implicit)
- Auto-reconnection logic (out of scope)

---

## Architecture: Dual-Channel Event System

### Current State

```rust
pub struct BiDiSession {
    ws_sink: Arc<TokioMutex<...>>,
    command_id: Arc<AtomicU64>,
    pending: Arc<StdMutex<HashMap<...>>>,
    event_tx: broadcast::Sender<BiDiEvent>,  // Single unified channel
}
```

Users receive all events and filter manually:
```rust
let mut rx = bidi.subscribe_events();
while let Ok(event) = rx.recv().await {
    if let BiDiEvent::Network(NetworkEvent::BeforeRequestSent(e)) = event {
        // handle
    }
}
```

### Enhanced Architecture

```rust
pub struct BiDiSession {
    // Existing fields (unchanged)
    ws_sink: Arc<TokioMutex<...>>,
    command_id: Arc<AtomicU64>,
    pending: Arc<StdMutex<HashMap<...>>>,
    event_tx: broadcast::Sender<BiDiEvent>,

    // NEW: Connection state
    connected: Arc<AtomicBool>,

    // NEW: Optional session-level timeout
    command_timeout: Option<Duration>,

    // NEW: Typed event channels (lazy-initialized)
    network_tx: OnceLock<broadcast::Sender<NetworkEvent>>,
    log_tx: OnceLock<broadcast::Sender<log::LogEvent>>,
    browsing_context_tx: OnceLock<broadcast::Sender<BrowsingContextEvent>>,
    script_tx: OnceLock<broadcast::Sender<ScriptEvent>>,
}
```

**Why dual-channel?**
- Unified channel preserves backward compatibility
- Typed channels provide ergonomic type-safe API
- Both channels receive the same events (dual broadcast)
- Users choose the pattern that fits their needs
- OnceLock ensures channels are created only when needed

---

## Event Subscription Patterns

Users can choose from three patterns based on their needs:

### Pattern 1: Unified Channel (Existing - Unchanged)

**API:**
```rust
pub fn subscribe_events(&self) -> broadcast::Receiver<BiDiEvent>
```

**Use case:** Users who want all events or multiple event types.

**Example:**
```rust
let mut rx = bidi.subscribe_events();
while let Ok(event) = rx.recv().await {
    match event {
        BiDiEvent::Network(e) => handle_network(e),
        BiDiEvent::Log(e) => handle_log(e),
        BiDiEvent::ConnectionClosed => break,
        _ => {}
    }
}
```

**Pros:**
- ✅ All events in one stream
- ✅ Already familiar to existing users
- ✅ Efficient for handling multiple event types

**Cons:**
- ❌ Requires manual pattern matching
- ❌ Easy to miss events if match is incomplete

---

### Pattern 2: Typed Receiver (Manual - New)

**API:**
```rust
pub fn network_events(&self) -> broadcast::Receiver<NetworkEvent>
pub fn log_events(&self) -> broadcast::Receiver<log::LogEvent>
pub fn browsing_context_events(&self) -> broadcast::Receiver<BrowsingContextEvent>
pub fn script_events(&self) -> broadcast::Receiver<ScriptEvent>
```

**Use case:** Users who want type safety but prefer explicit subscription control.

**Example:**
```rust
// Explicitly subscribe to network events
bidi.session().subscribe(&["network.*"], &[]).await?;

// Get typed receiver (no auto-subscribe)
let mut rx = bidi.network_events();
while let Ok(event) = rx.recv().await {
    // event is NetworkEvent - compile-time type safety
    match event {
        NetworkEvent::BeforeRequestSent(e) => { /* ... */ }
        NetworkEvent::ResponseCompleted(e) => { /* ... */ }
        _ => {}
    }
}
```

**Pros:**
- ✅ Type-safe - no manual BiDiEvent unwrapping
- ✅ Explicit control over subscriptions
- ✅ Can batch multiple domain subscriptions in one call

**Cons:**
- ❌ Two steps (subscribe + get receiver)
- ❌ Must remember to call session.subscribe() first

---

### Pattern 3: Auto-Subscribe (Convenience - New)

**API:**
```rust
pub async fn subscribe_network(&self) -> WebDriverResult<broadcast::Receiver<NetworkEvent>>
pub async fn subscribe_log(&self) -> WebDriverResult<broadcast::Receiver<log::LogEvent>>
pub async fn subscribe_browsing_context(&self) -> WebDriverResult<broadcast::Receiver<BrowsingContextEvent>>
pub async fn subscribe_script(&self) -> WebDriverResult<broadcast::Receiver<ScriptEvent>>
```

**Use case:** Users who want convenience - one call does everything.

**Example:**
```rust
// Automatically subscribes to network.* events
let mut rx = bidi.subscribe_network().await?;

while let Ok(event) = rx.recv().await {
    // event is NetworkEvent
    handle_network_event(event);
}
```

**Pros:**
- ✅ One call - subscribe + receiver
- ✅ Type-safe
- ✅ Convenient for common use cases

**Cons:**
- ❌ Hidden side effect (auto-subscribes to domain)
- ❌ Less efficient if subscribing to multiple domains

**Implementation:** Uses wildcard patterns (`"network.*"`, `"log.*"`, etc.) to subscribe to all events in that domain.

---

### Pattern Compatibility

All three patterns can be used simultaneously:
- Unified channel broadcasts to all subscribers
- Typed channels broadcast to their subscribers
- No conflicts or race conditions
- Events are fanned out to all active channels

---

## Timeout System

### Design Principle

**No default timeouts** - This is a library, not an application. Timeouts should be explicit and configurable. However, we provide clear documentation that **commands will hang indefinitely without timeouts** and recommend setting them.

### Three-Level Timeout System

#### Level 1: No Timeout (Default Behavior)

The existing `send_command()` method waits indefinitely unless a session-level timeout is configured.

```rust
// Current behavior - no timeout
let result = bidi.network().add_intercept(&[...]).await?;
```

**Use case:** Users who want full control or implement their own timeout wrapper.

#### Level 2: Session-Level Default (Configured via Builder)

Set a default timeout that applies to all commands on this session:

```rust
let bidi = driver.bidi_connect_builder()
    .command_timeout(Duration::from_secs(30))
    .connect()
    .await?;

// All commands now timeout after 30 seconds
let result = bidi.network().add_intercept(&[...]).await?;
```

**Use case:** Users who want a safety net for all commands.

#### Level 3: Per-Command Override

Override the session default for specific commands:

```rust
// Session has 30s default, but this command gets 60s
let result = bidi.send_command_with_timeout(
    "script.evaluate",
    params,
    Duration::from_secs(60)
).await?;
```

**Use case:** Slow operations that need more time than the session default.

### Implementation

```rust
impl BiDiSession {
    pub async fn send_command(&self, method: &str, params: Value) -> WebDriverResult<Value> {
        let (tx, rx) = oneshot::channel();
        // ... insert into pending, send to WebSocket ...

        match self.command_timeout {
            Some(timeout) => {
                tokio::time::timeout(timeout, rx)
                    .await
                    .map_err(|_| WebDriverError::BiDi(
                        format!("command '{}' timed out after {:?}", method, timeout)
                    ))??
            }
            None => rx.await?
        }
    }

    pub async fn send_command_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration
    ) -> WebDriverResult<Value> {
        let (tx, rx) = oneshot::channel();
        // ... insert into pending, send to WebSocket ...

        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| WebDriverError::BiDi(
                format!("command '{}' timed out after {:?}", method, timeout)
            ))??
    }
}
```

### Error Messages

Timeout errors include:
- The command method that timed out
- The timeout duration that was exceeded

Example: `BiDi error: command 'network.addIntercept' timed out after 30s`

---

## Builder Pattern

### BiDiSessionBuilder

```rust
pub struct BiDiSessionBuilder {
    event_channel_capacity: usize,
    command_timeout: Option<Duration>,
}

impl BiDiSessionBuilder {
    pub fn new() -> Self {
        Self {
            event_channel_capacity: 256,
            command_timeout: None,
        }
    }

    /// Set broadcast channel capacity for events (default: 256).
    pub fn event_channel_capacity(mut self, capacity: usize) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Set session-level command timeout.
    ///
    /// **IMPORTANT:** Without a timeout, commands can hang indefinitely.
    /// Recommended: `Duration::from_secs(30)`.
    pub fn command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = Some(timeout);
        self
    }

    /// Connect to BiDi WebSocket with configured options.
    pub async fn connect(self, ws_url: &str) -> WebDriverResult<BiDiSession> {
        // Implementation creates BiDiSession with builder values
    }
}

impl Default for BiDiSessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

### WebDriver Integration

```rust
impl WebDriver {
    /// Get a builder for custom BiDi connection configuration.
    pub fn bidi_connect_builder(&self) -> BiDiSessionBuilder {
        BiDiSessionBuilder::new()
    }

    // Existing convenience method unchanged
    pub async fn bidi_connect(&self) -> WebDriverResult<BiDiSession>
}
```

### Usage Examples

**Simple (no configuration):**
```rust
let bidi = driver.bidi_connect().await?;
```

**Configured:**
```rust
let bidi = driver.bidi_connect_builder()
    .command_timeout(Duration::from_secs(30))
    .event_channel_capacity(512)
    .connect()
    .await?;
```

**Production recommended:**
```rust
let bidi = driver.bidi_connect_builder()
    .command_timeout(Duration::from_secs(30))  // Safety net
    .event_channel_capacity(1024)              // Handle bursts
    .connect()
    .await?;
```

---

## Connection State Tracking

### API

```rust
impl BiDiSession {
    /// Check if the WebSocket connection is still alive.
    ///
    /// Returns `false` if the connection has closed. Note that this is a
    /// best-effort check - the connection could close immediately after
    /// this returns `true`.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}
```

### Implementation

**State management:**
- `connected: Arc<AtomicBool>` field on BiDiSession
- Set to `true` in `BiDiSession::connect()`
- Set to `false` in dispatch loop when WebSocket closes
- Emit `BiDiEvent::ConnectionClosed` when transitioning to false

**Usage:**
```rust
if !bidi.is_connected() {
    println!("Connection lost, reconnecting...");
    bidi = driver.bidi_connect().await?;
}
```

**Timing note:** This is a best-effort check. The connection could close immediately after returning `true`. Users should handle `ConnectionClosed` events for reliable detection.

---

## Enhanced Tracing

Add structured tracing throughout BiDi operations for observability:

### Connection Lifecycle

```rust
// In BiDiSession::connect()
tracing::debug!(url = %ws_url, "BiDi WebSocket connected");

// In dispatch loop on close
tracing::debug!("BiDi WebSocket connection closed");
```

### Command Tracing

```rust
// In send_command() before sending
tracing::trace!(
    method = %method,
    id = %id,
    params = ?params,
    "Sending BiDi command"
);

// In dispatch loop on response
tracing::trace!(
    id = %id,
    success = %is_success,
    "Received BiDi response"
);
```

### Event Tracing

```rust
// In dispatch loop when event received
tracing::trace!(
    method = %method,
    "Received BiDi event"
);
```

### Dispatch Loop Span

```rust
let span = tracing::debug_span!("bidi_dispatch", url = %ws_url);
tokio::spawn(async move {
    // ... dispatch loop
}.instrument(span));
```

**Trace levels:**
- `debug` - Connection lifecycle events
- `trace` - Individual commands, responses, events
- `error` - WebSocket errors (already present)
- `warn` - JSON parse failures (already present)

**Why trace/debug?** These levels are typically off in production, so there's no performance impact unless users explicitly enable BiDi tracing.

---

## Documentation Strategy

### 1. Timeout Documentation

**Module-level docs in `mod.rs`:**

```rust
//! ## Command Timeouts
//!
//! **IMPORTANT:** By default, BiDi commands have NO timeout and can hang indefinitely
//! if the browser or connection fails. It is **highly recommended** to configure a timeout:
//!
//! ```no_run
//! use std::time::Duration;
//! use thirtyfour::prelude::*;
//!
//! # async fn example() -> WebDriverResult<()> {
//! # let driver = todo!();
//! let bidi = driver.bidi_connect_builder()
//!     .command_timeout(Duration::from_secs(30))
//!     .connect()
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
//! let result = bidi.send_command_with_timeout(
//!     "script.evaluate",
//!     serde_json::json!({"expression": "longRunningFunction()"}),
//!     Duration::from_secs(60)  // Override session default
//! ).await?;
//! # Ok(())
//! # }
//! ```
```

**Method-level docs:**

```rust
impl BiDiSessionBuilder {
    /// Set a session-level default timeout for all commands.
    ///
    /// **IMPORTANT:** Without a timeout, commands can hang indefinitely if the
    /// browser or connection fails. It is highly recommended to set a timeout
    /// (e.g., `Duration::from_secs(30)`).
    ///
    /// You can override this per-command using `send_command_with_timeout()`.
    pub fn command_timeout(mut self, timeout: Duration) -> Self
}
```

### 2. Event Subscription Documentation

**Module-level docs in `mod.rs`:**

```rust
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
//!         BiDiEvent::Network(e) => handle_network(e),
//!         BiDiEvent::Log(e) => handle_log(e),
//!         BiDiEvent::ConnectionClosed => break,
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! # fn handle_network(e: thirtyfour::extensions::bidi::NetworkEvent) {}
//! # fn handle_log(e: thirtyfour::extensions::bidi::log::LogEvent) {}
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

### 3. Integration Test Documentation

**Create `tests/bidi_integration_tests.md`:**

```markdown
# BiDi Integration Tests

## Requirements

- Chrome 115+ or Firefox 119+ with BiDi support
- Running WebDriver server (chromedriver/geckodriver)

## Browser Setup

### Chrome with Chromedriver

```bash
chromedriver --port=9515
```

### Firefox with Geckodriver

```bash
geckodriver --port=4444
```

## Running Tests

```bash
# Run all BiDi integration tests
cargo test --test bidi --features bidi

# Run specific test
cargo test --test bidi --features bidi test_network_intercept

# Run with trace logging
RUST_LOG=thirtyfour::extensions::bidi=trace cargo test --test bidi --features bidi
```

## Writing Integration Tests

Integration tests should:
- Use `#[tokio::test]` for async tests
- Set `webSocketUrl: true` in capabilities for Chrome
- Handle cleanup in case of test failure
- Test one feature per test function
- Document browser-specific behaviors

Example test structure:

```rust
#[tokio::test]
async fn test_network_intercept() -> WebDriverResult<()> {
    let mut caps = DesiredCapabilities::chrome();
    caps.insert_base_capability("webSocketUrl".into(), true.into());

    let driver = WebDriver::new("http://localhost:9515", caps).await?;
    let bidi = driver.bidi_connect_builder()
        .command_timeout(Duration::from_secs(10))
        .connect()
        .await?;

    // Test network interception
    let mut rx = bidi.subscribe_network().await?;
    // ... test code ...

    driver.quit().await?;
    Ok(())
}
```
```

### 4. README.md Update

Add to the "Feature Flags" section:

```markdown
- `bidi`: Enable WebDriver BiDi (bidirectional protocol) support for real-time
  event handling, network interception, script evaluation, and more.
  Requires Chrome 115+ or Firefox 119+. See `examples/bidi_network_intercept.rs`
  for usage examples.
```

---

## Implementation Impact

### Files to Modify

| File | Changes |
|------|---------|
| `src/extensions/bidi/mod.rs` | Add fields, builder, typed subscription methods, timeout logic, tracing |
| `src/web_driver.rs` | Add `bidi_connect_builder()` method |
| `README.md` | Update feature flags section |
| `tests/bidi_integration_tests.md` | Create new documentation file |

### Breaking Changes

**None.** All changes are additive:
- Existing `subscribe_events()` unchanged
- Existing `bidi_connect()` unchanged
- New fields are internal implementation details
- Builder is a new API, doesn't affect existing code

### Performance Impact

**Minimal:**
- Dual broadcast adds one extra `send()` per event (microseconds)
- OnceLock has zero overhead after initialization
- AtomicBool reads are lock-free
- Tracing is compile-time disabled unless explicitly enabled

**Memory:**
- 4 OnceLock fields (16 bytes each = 64 bytes)
- Each typed channel when initialized: ~1KB
- Total: < 5KB per BiDiSession

---

## Testing Strategy

### Unit Tests

Test each new component in isolation:

```rust
#[cfg(test)]
mod tests {
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
    fn test_is_connected_initial_state() {
        // Test that connected starts as false before connect()
    }
}
```

### Integration Tests

Test real browser interaction:

```rust
#[tokio::test]
async fn test_typed_subscription() -> WebDriverResult<()> {
    let driver = setup_chrome_with_bidi().await?;
    let bidi = driver.bidi_connect().await?;

    let mut rx = bidi.subscribe_network().await?;
    driver.goto("https://example.com").await?;

    let event = rx.recv().await.expect("Should receive network event");
    // Verify event is NetworkEvent type

    driver.quit().await?;
    Ok(())
}

#[tokio::test]
async fn test_command_timeout() -> WebDriverResult<()> {
    let driver = setup_chrome_with_bidi().await?;
    let bidi = driver.bidi_connect_builder()
        .command_timeout(Duration::from_millis(1))  // Very short timeout
        .connect()
        .await?;

    // This should timeout
    let result = bidi.network().add_intercept(&[InterceptPhase::BeforeRequestSent]).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("timed out"));

    driver.quit().await?;
    Ok(())
}
```

---

## Success Criteria

1. ✅ Users can choose from 3 subscription patterns
2. ✅ Timeout configuration works at session and command levels
3. ✅ `is_connected()` accurately reflects connection state
4. ✅ Tracing provides visibility into BiDi operations
5. ✅ Builder pattern allows pre-connection configuration
6. ✅ Zero breaking changes to existing API
7. ✅ Documentation clearly explains timeout importance
8. ✅ All existing tests pass
9. ✅ New integration tests cover typed subscriptions and timeouts
10. ✅ Clippy passes with `-D warnings`

---

## Future Enhancements (Out of Scope)

These are explicitly NOT part of this design but could be considered later:

- Auto-reconnection logic on connection loss
- Event replay/caching for late subscribers
- Metrics/statistics on command latency
- Built-in retry logic for transient failures
- Connection pooling for multiple sessions
