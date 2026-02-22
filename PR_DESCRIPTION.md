# feat(bidi): Add WebDriver BiDi Protocol Support

## Summary

This PR introduces full WebDriver BiDi (Bidirectional) protocol support to thirtyfour,
enabling real-time two-way communication with the browser over a WebSocket connection.
It also removes the `selenium-manager` feature (which required a workspace-local path
dependency) and modernises all crate dependencies.

---

## What is WebDriver BiDi?

WebDriver BiDi is a new W3C specification for browser automation that supplements the
classic HTTP-based WebDriver protocol with a persistent WebSocket connection. This
enables:

- **Real-time event streaming** — receive network, log, script, and browsing context
  events as they happen, without polling.
- **Network interception** — inspect and modify HTTP requests and responses in-flight.
- **Script execution** — evaluate JavaScript in browser realms with structured results.
- **Preload scripts** — inject JavaScript that runs before every page load.

Supported browsers: Chrome 115+, Firefox 119+, Edge 115+.

---

## Changes

### New: `bidi` Feature Flag

Enable with `features = ["bidi"]` in your `Cargo.toml`:

```toml
thirtyfour = { version = "0.36", features = ["bidi"] }
```

### New Public API

#### `BiDiSession` and `BiDiSessionBuilder`

```rust
// Simple connection (ws://)
let mut bidi = driver.bidi_connect().await?;
tokio::spawn(bidi.dispatch_future().expect("dispatch already started"));

// Builder pattern — TLS, auth, timeouts, capacity
let mut bidi = BiDiSessionBuilder::new()
    .install_crypto_provider()          // required for wss:// connections
    .basic_auth("user", "pass")         // HTTP Basic Auth for Selenium Grid
    .command_timeout(Duration::from_secs(10))
    .event_channel_capacity(512)
    .connect_with_driver(&driver)
    .await?;
tokio::spawn(bidi.dispatch_future().expect("dispatch already started"));
```

#### Dispatch Loop (Runtime-Flexible)

The dispatch loop is **not** spawned automatically. Users control the lifecycle:

```rust
// Option 1: spawn on tokio (recommended)
tokio::spawn(bidi.dispatch_future().expect("dispatch already started"));

// Option 2: manual polling for custom loops
loop {
    let more = bidi.poll_dispatch().await?;
    if !more { break; }
}
```

#### Event Subscription — Three Patterns

**Unified channel** (all events):
```rust
let mut rx = bidi.subscribe_events();
while let Ok(event) = rx.recv().await {
    match event {
        BiDiEvent::Network(e) => { /* ... */ }
        BiDiEvent::Log(e)     => { /* ... */ }
        BiDiEvent::ConnectionClosed => break,
        _ => {}
    }
}
```

**Typed channels** (domain-specific):
```rust
let mut network_rx = bidi.network_events();
let mut log_rx     = bidi.log_events();
```

**Auto-subscribe** (convenience — calls `session.subscribe` and returns receiver):
```rust
let mut rx = bidi.subscribe_network().await?;
// subscribe_log(), subscribe_browsing_context(), subscribe_script() also available
```

#### Domain Accessors

| Method | Domain | Key operations |
|---|---|---|
| `bidi.session()` | `session` | `subscribe`, `unsubscribe` |
| `bidi.network()` | `network` | `add_intercept`, `remove_intercept`, `continue_request`, `continue_response`, `fail_request`, `provide_response` |
| `bidi.log()` | `log` | `subscribe` |
| `bidi.browsing_context()` | `browsingContext` | `get_tree`, `create`, `close`, `navigate`, `reload`, `activate`, `set_viewport`, `capture_screenshot`, `traverse_history` |
| `bidi.script()` | `script` | `evaluate`, `call_function`, `add_preload_script`, `remove_preload_script`, `disown` |
| `bidi.browser()` | `browser` | `close`, `create_user_context`, `remove_user_context`, `get_user_contexts` |
| `bidi.console()` | `console` (log alias) | `subscribe` |
| `bidi.input()` | `input` | `perform`, `release`, `set_files` |
| `bidi.permissions()` | `permissions` | `set_permission` |
| `bidi.storage()` | `storage` | `get_cookies`, `set_cookie`, `delete_cookies` |
| `bidi.webextension()` | `webExtension` | `install`, `install_from_directory`, `install_from_file`, `install_encoded`, `uninstall` |
| `bidi.emulation()` | `emulation` | `set_geolocation_override`, `set_device_posture` |
| `bidi.cdp()` | `cdp` (passthrough) | `send_command`, `resolve_realm` |

#### `WebDriver` convenience methods

```rust
driver.bidi_connect()                              // simple ws:// connection
driver.bidi_connect_with_builder(builder)          // full builder configuration
```

#### Connection State

```rust
if bidi.is_connected() {
    bidi.send_command("some.method", json!({})).await?;
}
bidi.is_dispatch_started()  // true after dispatch_future() is called
```

#### Per-Command Timeout Override

```rust
bidi.send_command_with_timeout("network.addIntercept", params, Duration::from_secs(5)).await?;
```

---

### Infrastructure Changes

#### Removed: `selenium-manager` feature

The `selenium-manager` feature and `web_driver_process` module have been removed.
The dependency pointed to a workspace-local path (`../selenium/rust`) that does not
exist in a standalone checkout of this repository. This caused the crate to fail
compilation entirely when the feature was enabled (the default), which blocked all
development, testing, and CI validation of any changes to the library. Removing it
was a prerequisite for the codebase to be buildable and testable at all.

Users who relied on `start_webdriver_process` should manage their WebDriver process
externally (e.g., via `std::process::Command` or a dedicated tool).

**BREAKING**: `start_webdriver_process`, `start_webdriver_process_full`,
`WebDriverProcess`, `WebDriverProcessBrowser`, `WebDriverProcessPort` are no longer
exported.

#### Renamed feature: `rustls-tls` → `rustls`

The TLS feature flag has been renamed from `rustls-tls` to `rustls` to align with
the updated reqwest 0.13 API. The feature is still enabled by default.

**BREAKING**: Update `features = ["rustls-tls"]` to `features = ["rustls"]` if you
were specifying it explicitly (it remains in `default`).

#### Dependency updates

| Crate | From | To |
|---|---|---|
| `reqwest` | 0.12.8 | 0.13.2 |
| `tokio` | 1.30 | 1.49 |
| `serde` | 1.0.210 | 1.0.228 |
| `serde_json` | 1.0.132 | 1.0.149 |
| `thiserror` | 2.0.12 | 2.0.18 |
| `pastey` | 0.1 | 0.2 |
| `async-trait` | 0.1.83 | 0.1.89 |
| `futures-util` | 0.3.31 | 0.3.32 |

New optional dependencies (only with `bidi` feature):
- `tokio-tungstenite` 0.28 (WebSocket client)
- `rustls` 0.23 with `aws_lc_rs` (TLS for `wss://`)
- `pin-project-lite` 0.2 (zero-overhead `DispatchFuture` projection)

#### New: `WebDriverError::BiDi` variant

```rust
WebDriverError::BiDi(String)
```

Used for all BiDi-specific errors (connection failures, protocol errors,
response parsing failures).

---

## Breaking Changes Summary

1. `start_webdriver_process` and related symbols removed (selenium-manager removed).
2. Feature `rustls-tls` renamed to `rustls`.
3. `BiDiSession::dispatch_future()` now returns `Option<DispatchFuture>` — callers
   must call `.expect()` or handle `None` (returned if dispatch was already started).
   Previously the dispatch loop was spawned automatically inside `connect()`.

---

## Design Decisions

### Runtime Flexibility

The dispatch future is decoupled from the connection, allowing users to choose their
own runtime (tokio, async-std, etc.) and control the background task lifecycle.
When `driver.quit()` is called, the WebSocket closes and `dispatch_future()` completes
automatically, so no explicit cleanup of the BiDi session is required.

### No Hidden Threads

Zero background threads are spawned by the library. The dispatch future is a
`pin_project!` struct implementing `Future` directly over the WebSocket stream,
with no hidden `tokio::spawn` calls. Only `install_crypto_provider()` has a
process-global side effect (installing the rustls crypto provider), and this is
**opt-in** and documented.

### Panics Eliminated

All `.unwrap()` calls in the BiDi implementation have been replaced with safe
alternatives. Mutex poisoning is handled gracefully. JSON field access uses `.get()`
with proper error propagation. The library cannot panic in downstream user code.

### `#[non_exhaustive]` on `BiDiEvent`

`BiDiEvent` is marked `#[non_exhaustive]` to allow adding new event variants in
future versions without breaking existing `match` expressions. Users must handle
the `_` case.

---

## WebExtension Notes

The `webExtension.install` API follows the WebDriver BiDi specification:

| Method | BiDi type | Works in Chrome | Works on Grid |
|---|---|---|---|
| `install_from_directory(path)` | `path` | Yes (needs flags) | No (path must be on node) |
| `install(archive_path)` | `archivePath` | No (not implemented) | No |
| `install_from_file(path)` | `base64` | No (not implemented) | No |
| `install_encoded(b64)` | `base64` | No (not implemented) | No |

ChromeDriver only supports `path` type as of Chrome 136. For Grid or cross-machine
scenarios, load extensions via `ChromiumCapabilities::add_extension()` at session
creation time instead.

---

## Example

See `examples/bidi_network_intercept.rs`:

```bash
cargo run --example bidi_network_intercept --features bidi
```

---

## Verification

- `cargo build --lib --features bidi` — clean ✅
- `cargo clippy --lib --features bidi` — clean (zero warnings) ✅
- `cargo doc --features bidi --no-deps` — clean (zero warnings) ✅
- `cargo test` (unit tests) — all pass ✅
- No TODOs introduced (pre-existing `// TODO: create builder` in `web_driver.rs` is
  from the original codebase) ✅
- No hidden thread spawning ✅
- No panics in production code paths ✅

---

## Checklist

- [x] New BiDi feature behind `--features bidi` — no impact on non-BiDi users
- [x] All new public APIs documented with `# Errors` sections
- [x] `#[must_use]` on all builder methods and `dispatch_future()`
- [x] `BiDiEvent` is `#[non_exhaustive]` for forward compatibility
- [x] README updated: feature flags, example updated, BiDi example added
- [x] `native-tls` feature correctly mapped to `reqwest/native-tls`
- [x] Unit tests for builder, error variant, session response parsing
- [x] Integration test documentation in `tests/bidi_integration_tests.md`
