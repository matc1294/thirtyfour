# BiDi Grid Connection Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix BiDi connection to use either the given wss route by Selenium Grid or reuse/rederive the grid URL, with proper handling for HTTP Basic Authentication and schema matching (ws vs wss).

**Architecture:** Extend `BiDiSessionBuilder` to support:
1. Direct ws_url specification when user knows the WebSocket endpoint
2. Automatic derivation from driver server_url when webSocketUrl not in capabilities
3. Schema validation: use `wss://` for external connections, `ws://` for internal

**Tech Stack:** Rust/Tokio, tokio-tungstenite, URL parsing with `url` crate

---

### Task 1: Add ws_url Derivation from Driver Server URL (Fallback)

**Files:**
- Modify: `thirtyfour/thirtyfour/src/session/handle.rs`
- Modify: `thirtyfour/thirtyfour/src/web_driver.rs`

**Step 1: Write failing test**

In `thirtyfour/thirtyfour/tests/bidi_tests.rs` (create if doesn't exist):

```rust
#[cfg(feature = "bidi")]
mod tests {
    use thirtyfour::extensions::bidi::{BiDiSessionBuilder, BiDiConnectionUrl};
    
    #[test]
    fn test_derive_ws_url_from_http_hub() {
        // When server is http://localhost:4444, derive ws://localhost:4444
        let derived = BiDiConnectionUrl::from_server_url(
            "http://localhost:4444/wd/hub"
        );
        assert_eq!(derived.unwrap(), "ws://localhost:4444");
    }
    
    #[test]
    fn test_derive_wss_from_https_hub() {
        // When server is https://grid.example.com, derive wss://grid.example.com
        let derived = BiDiConnectionUrl::from_server_url(
            "https://grid.example.com/wd/hub"
        );
        assert_eq!(derived.unwrap(), "wss://grid.example.com");
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cd thirtyfour && cargo test --package thirtyfour --features bidi bidi_tests -v 2>&1 | tail
```
Expected: FAIL (BiDiConnectionUrl not defined)

**Step 3: Add BiDiConnectionUrl struct and from_server_url method**

In `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`, add after existing imports:

```rust
use url::Url;

/// Configuration for BiDi WebSocket connection URL.
///
/// Supports multiple ways to specify the connection:
/// - Use the webSocketUrl returned in session capabilities (default)
/// - Derive from the Selenium hub server URL
/// - Specify a custom WebSocket URL directly
#[derive(Debug, Clone)]
pub enum BiDiConnectionUrl {
    /// Direct WebSocket URL (ws:// or wss://)
    Direct(String),
    /// Derive from the Selenium hub server URL
    DerivedFromHub { host: String, port: u16 },
}

impl BiDiConnectionUrl {
    /// Create a BiDi connection URL by deriving from the Selenium hub server URL.
    ///
    /// This takes the HTTP/HTTPS scheme and host:port from the hub
    /// and converts it to ws:/wss:// for WebSocket connections.
    ///
    /// Note: The path is not derived because the actual BiDi path
    /// is returned by the browser at session creation time.
    pub fn from_server_url(server_url: &str) -> WebDriverResult<Self> {
        let url = Url::parse(server_url)
            .map_err(|e| WebDriverError::ParseError(format!("invalid server URL: {e}")))?;
        
        let scheme = match url.scheme() {
            "http" => "ws",
            "https" => "wss",
            s => return Err(WebDriverError::ParseError(
                format!("unsupported scheme '{}', expected http or https", s)
            )),
        };
        
        let host = url.host_str()
            .ok_or_else(|| WebDriverError::ParseError("URL missing host".to_string()))?;
        
        // Build ws://host:port (without path - path comes from browser)
        let port = url.port().unwrap_or(match scheme {
            "ws" => 80,
            "wss" => 443,
            _ => 80,
        });
        
        Ok(Self::Direct(format!("{}://{}:{}", scheme, host, port)))
    }
    
    /// Convert to full WebSocket URL string.
    pub fn to_websocket_url(&self) -> String {
        match self {
            Self::Direct(url) => url.clone(),
            Self::DerivedFromHub { .. } => unimplemented!(),
        }
    }
}

impl From<String> for BiDiConnectionUrl {
    fn from(s: String) -> Self {
        Self::Direct(s)
    }
}
```

**Step 4: Run test to verify it compiles and passes**

```bash
cd thirtyfour && cargo test --package thirtyfour --features bidi bidi_tests -v 2>&1 | tail
```
Expected: PASS

**Step 5: Commit**

```bash
git add thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add BiDiConnectionUrl for flexible ws URL configuration"
```

---

### Task 2: Add Custom WebSocket URL Support to BiDiSessionBuilder

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Write failing test**

In existing bidi tests file:

```rust
#[test]
fn test_bidi_builder_with_custom_url() {
    let builder = BiDiSessionBuilder::new()
        .websocket_url("ws://custom-host:9222/devtools");
    
    assert_eq!(builder.websocket_url, Some(BiDiConnectionUrl::Direct(
        "ws://custom-host:9222/devtools".to_string()
    )));
}
```

**Step 2: Run test to verify it fails**

```bash
cd thirtyfour && cargo test --package thirtyfour --features bidi bidi_tests -v 2>&1 | tail
```
Expected: FAIL (websocket_url field/method not defined)

**Step 3: Add websocket_url field and method to BiDiSessionBuilder**

In `BiDiSessionBuilder` struct:

```rust
#[derive(Debug, Clone)]
pub struct BiDiSessionBuilder {
    pub(crate) event_channel_capacity: usize,
    pub(crate) command_timeout: Option<Duration>,
    pub(crate) install_crypto_provider: bool,
    pub(crate) basic_auth: Option<(String, String)>,
    /// Optional custom WebSocket URL or configuration for deriving it.
    pub(crate) websocket_url: Option<BiDiConnectionUrl>,
}
```

Update Default impl:

```rust
impl Default for BiDiSessionBuilder {
    fn default() -> Self {
        Self {
            event_channel_capacity: 256,
            command_timeout: None,
            install_crypto_provider: false,
            basic_auth: None,
            websocket_url: None,
        }
    }
}
```

Add builder method:

```rust
/// Set a custom WebSocket URL for the BiDi connection.
///
/// Use this when:
/// - The Selenium Grid provides a specific WebSocket route
/// - You want to use a different host than returned in capabilities
///
/// # Example
///
/// ```no_run
/// let bidi = BiDiSessionBuilder::new()
///     .websocket_url("ws://grid-node:9222/session/{session}/se/bidi")
///     .connect_with_driver(&driver)
///     .await?;
/// ```
#[must_use]
pub fn websocket_url(mut self, url: impl Into<BiDiConnectionUrl>) -> Self {
    self.websocket_url = Some(url.into());
    self
}
```

**Step 4: Run test to verify it passes**

```bash
cd thirtyfour && cargo test --package thirtyfour --features bidi bidi_tests -v 2>&1 | tail
```
Expected: PASS

**Step 5: Commit**

```bash
git add thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): add custom websocket_url to BiDiSessionBuilder"
```

---

### Task 3: Update connect_with_driver to Use Custom URL or Fallback Logic

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_connect_with_driver_uses_custom_url_when_set() {
    // This tests that when websocket_url is set on builder,
    // it takes precedence over driver.handle.websocket_url
}
```

**Step 2: Run test to verify fails (if we can compile without running)**

Actually, let's update the connect_with_driver implementation:

```rust
/// Connect using `WebDriver`'s session capabilities or custom URL.
///
/// This method respects all builder configuration including:
/// - `install_crypto_provider()` for TLS connections
/// - `basic_auth()` for HTTP Basic Authentication  
/// - `command_timeout()` for command timeouts
/// - `event_channel_capacity()` for event buffer size
/// - `websocket_url()` to override the default capability-derived URL
///
/// Priority order:
/// 1. Custom WebSocket URL set via `websocket_url()`
/// 2. webSocketUrl from session capabilities (via driver)
/// 3. Derive from driver's server_url if neither above available
pub async fn connect_with_driver(
    self,
    driver: &crate::WebDriver,
) -> WebDriverResult<BiDiSession> {
    let ws_url = if let Some(ref custom_url) = self.websocket_url {
        // User specified custom URL - use it directly
        custom_url.to_websocket_url()
    } else if let Some(ref capability_url) = driver.handle.websocket_url {
        // Use URL from session capabilities
        capability_url.clone()
    } else {
        // Fallback: try to derive from hub server URL
        let hub_url = driver.handle.server_url.to_string();
        BiDiConnectionUrl::from_server_url(&hub_url)
            .map_err(|e| WebDriverError::BiDi(format!("Failed to derive ws URL: {e})))?
            .to_websocket_url()
    };
    
    self.connect(&ws_url).await
}
```

**Step 3: Run test to verify it compiles**

```bash
cd thirtyfour && cargo check --package thirtyfour --features bidi 2>&1 | tail -20
```
Expected: PASS

**Step 4: Commit**

```bash
git add thirtyfour/src/extensions/bidi/mod.rs
git commit -m "feat(bidi): update connect_with_driver to support custom URL and fallback"
```

---

### Task 4: Add HTTP Basic Auth Support for WebSocket Connections (Deploy Pattern)

**Files:**
- Modify: `thirtyfour/thirtyfour/src/extensions/bidi/mod.rs` (already has basic_auth)

Verify existing implementation handles auth correctly:
- Already implemented in BiDiSessionBuilder::basic_auth()
- Needs to be used when connecting

Let me check current connect_with_config implementation:

```rust
// Around line 594 - verify it calls connect_with_auth when basic_auth is set
if let Some((ref username, ref password)) = config.basic_auth {
    Self::connect_with_auth(ws_url, username, password).await?
} else {
    // existing plain connection code
}
```

This should already work. Let's add a test to verify:

**Step 1: Write integration test (conceptual - will need actual running grid)**

```rust
#[tokio::test]
#[ignore] // Requires Selenium Grid with auth
async fn test_bidi_with_basic_auth() {
    // This would require a real grid with Basic Auth configured
}
```

**Step 2: Commit**

If tests are passing, commit:

```bash
git add thirtyfour/src/extensions/bidi/mod.rs
git commit -m "test(bidi): verify basic auth works for bidi connections"
```

---

### Task 5: Update WebDriver to Expose server_url (Needed for Derivation)

**Files:**
- Modify: `thirtyfour/thirtyfour/src/session/handle.rs`

The BiDiConnectionUrl::from_server_url needs access to the driver's server URL.
Let's verify it's accessible:

```rust
// In SessionHandle, check if we have a public method or field:
// Currently server_url is: pub server_url: Arc<Url>
```

It appears server_url may not be publicly exposed. Let's add an accessor:

**Step 1: Write failing test**

```rust
#[test]
fn test_session_handle_server_url_accessible() {
    // Need to verify we can access server_url from BiDi code
}
```

**Step 2: Add getter method if needed**

In `session/handle.rs`:

```rust
/// Get the WebDriver server URL.
pub fn server_url(&self) -> &Url {
    &self.server_url
}
```

**Step 3: Verify compilation**

```bash
cd thirtyfour && cargo check --package thirtyfour --features bidi 2>&1 | tail -10
```
Expected: PASS

**Step 4: Commit**

```bash
git add thirtyfour/src/session/handle.rs
git commit -m "feat(session): expose server_url getter for BiDi URL derivation"
```

---

### Task 6: Verify Schema Matching (ws vs wss)

The implementation in Task 1 already handles this:

- `http://` → `ws://`
- `https://` → `wss://`

Let's add a test to verify:

**Step 1: Add tests for schema matching**

```rust
#[test]
fn test_schema_matching_http_to_ws() {
    let url = BiDiConnectionUrl::from_server_url("http://localhost:4444").unwrap();
    assert!(url.to_websocket_url().starts_with("ws://"));
}

#[test]  
fn test_schema_matching_https_to_wss() {
    let url = BiDiConnectionUrl::from_server_url("https://grid.example.com").unwrap();
    assert!(url.to_websocket_url().starts_with("wss://"));
}
```

**Step 2: Run tests**

```bash
cd thirtyfour && cargo test --package thirtyfour --features bidi schema_matching -v 2>&1 | tail
```
Expected: PASS

**Step 3: Commit**

```bash
git add thirtyfour/src/extensions/bidi/mod.rs
git commit -m "test(bidi): verify ws/wss schema matching"
```

---

### Task 7: Run Full Test Suite and Lint

**Files:**
- All modified files

**Step 1: Run linting**

```bash
cd thirtyfour && cargo clippy --package thirtyfour --features bidi 2>&1 | head -30
```
Fix any issues reported.

**Step 2: Run tests**

```bash
cd thirtyfour && cargo test --package thirtyfour --features bidi 2>&1 | tail -40
```

Expected: All pass

**Step 3: Commit**

```bash
git add .
git commit -m "fix(bidi): improve grid connection with custom URL, auth, and schema matching"
```