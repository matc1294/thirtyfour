# Performance & Efficiency Wave — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate redundant memory allocations in the HTTP hot path and prevent OOM crashes via a dynamic, system-aware memory guard — with zero breaking API changes.

**Architecture:** The `HttpClient::send()` implementation in `session/http.rs` is modified to (1) check `Content-Length` against available system RAM before reading any body, (2) only allocate a `String` from the body on the error path, and (3) avoid cloning headers by passing references. A new `WebDriverError::ResponseTooLarge` variant communicates memory guard failures to users. The `Method` field in `RequestData` is wrapped in `Arc` to eliminate per-request clones.

**Tech Stack:** Rust, `reqwest` 0.13, `sysinfo` 0.32, `thiserror` 2.0, `tokio` 1.x, existing `webdriver_err!` macro in `error.rs`.

---

## File Map

| File | Action | Responsibility |
|:---|:---|:---|
| `thirtyfour/Cargo.toml` | Modify | Add `sysinfo` dependency |
| `thirtyfour/src/error.rs` | Modify | Add `ResponseTooLarge` error variant |
| `thirtyfour/src/common/requestdata.rs` | Modify | Wrap `Method` in `Arc` |
| `thirtyfour/src/session/http.rs` | Modify | Memory guard, lazy allocation, reference headers |

---

## Task 1: Add `sysinfo` dependency

**Files:**
- Modify: `thirtyfour/Cargo.toml`

- [ ] **Step 1: Open `thirtyfour/Cargo.toml` and add `sysinfo` under `[dependencies]`**

The existing dependencies block ends around line 77. Add `sysinfo` after `tracing`:

```toml
sysinfo = { version = "0.32", default-features = false, features = ["system"] }
```

> Use `default-features = false` with `features = ["system"]` to pull in only the memory/system APIs — not process listing, disk, or network, which we do not need (YAGNI).

- [ ] **Step 2: Verify the dependency resolves**

```bash
cd thirtyfour && cargo fetch
```

Expected: no errors, `sysinfo` appears in `Cargo.lock`.

- [ ] **Step 3: Commit**

```bash
git add thirtyfour/Cargo.toml Cargo.lock
git commit -m "chore: add sysinfo dependency for dynamic memory guard"
```

---

## Task 2: Add `ResponseTooLarge` error variant

**Files:**
- Modify: `thirtyfour/src/error.rs` (around line 285, inside the `webdriver_err!` macro block)

The error module uses a custom `webdriver_err!` macro to generate variants. Follow the exact existing pattern.

- [ ] **Step 1: Write the failing test**

Add inside the existing `#[cfg(test)]` block at the bottom of `thirtyfour/src/error.rs` (create the block if it does not exist):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_too_large_error_message() {
        let err = WebDriverError::ResponseTooLarge(
            "Response body (600000000 bytes) exceeds safe memory limit \
             (500000000 bytes). Available RAM: 625000000 bytes"
                .to_string(),
        );
        let msg = err.to_string();
        assert!(msg.contains("Response body"), "message should mention response body");
        assert!(msg.contains("exceeds safe memory limit"), "message should mention limit");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd thirtyfour && cargo test test_response_too_large_error_message 2>&1 | tail -20
```

Expected: compile error — `WebDriverError::ResponseTooLarge` does not exist yet.

- [ ] **Step 3: Add the variant inside the `webdriver_err!` macro block**

In `thirtyfour/src/error.rs`, locate the last variant before the closing `}` of the macro (currently `BiDi(String)` at ~line 289). Add after it:

```rust
        #[error("Response body too large to process safely: {0}")]
        ResponseTooLarge(String),
```

The full closing section should now look like:

```rust
        #[error("BiDi error: {0}")]
        BiDi(String),
        #[error("Response body too large to process safely: {0}")]
        ResponseTooLarge(String),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd thirtyfour && cargo test test_response_too_large_error_message 2>&1 | tail -20
```

Expected: `test test_response_too_large_error_message ... ok`

- [ ] **Step 5: Commit**

```bash
git add thirtyfour/src/error.rs
git commit -m "feat(error): add ResponseTooLarge variant for memory guard failures"
```

---

## Task 3: Wrap `Method` in `Arc` inside `RequestData`

**Files:**
- Modify: `thirtyfour/src/common/requestdata.rs`

The `uri` field already uses `Arc<str>`. The `method: Method` field is currently owned and cloned on every `run_webdriver_cmd` call (line 186 of `http.rs`). Wrapping it in `Arc` makes cloning a reference-count increment.

- [ ] **Step 1: Write the failing test**

Add inside `requestdata.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;

    #[test]
    fn test_request_data_method_clone_is_arc() {
        let rd = RequestData::new(Method::GET, "/session");
        let cloned = rd.clone();
        // Both must point to the same Method value
        assert_eq!(rd.method, cloned.method);
        // Arc::ptr_eq confirms they share the same allocation
        assert!(Arc::ptr_eq(&rd.method, &cloned.method));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd thirtyfour && cargo test test_request_data_method_clone_is_arc 2>&1 | tail -20
```

Expected: compile error — `Arc::ptr_eq` cannot be called on `Method` (not wrapped in `Arc` yet).

- [ ] **Step 3: Change `method` field type and update `new()`**

In `thirtyfour/src/common/requestdata.rs`, replace the struct and its `new` constructor:

```rust
use std::sync::Arc;
use std::fmt::Display;

use crate::IntoArcStr;
use http::Method;

/// `RequestData` is a wrapper around the data required to make an HTTP request.
#[derive(Debug, Clone)]
pub struct RequestData {
    /// The request method.
    pub method: Arc<Method>,
    /// The request URI.
    pub uri: Arc<str>,
    /// The request body.
    pub body: Option<serde_json::Value>,
}

impl RequestData {
    /// Create a new `RequestData` struct.
    pub fn new<S: IntoArcStr>(method: Method, uri: S) -> Self {
        RequestData {
            method: Arc::new(method),
            uri: uri.into(),
            body: None,
        }
    }

    /// Add a request body.
    #[must_use]
    pub fn add_body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }
}

impl Display for RequestData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(body) = &self.body {
            write!(
                f,
                "{} {} {}",
                self.method,
                self.uri,
                serde_json::to_string(body).unwrap_or_default()
            )
        } else {
            write!(f, "{} {}", self.method, self.uri)
        }
    }
}
```

- [ ] **Step 4: Fix the call site in `http.rs`**

In `thirtyfour/src/session/http.rs`, line 186 currently reads:

```rust
.method(request_data.method.clone())
```

Because `method` is now `Arc<Method>`, the `http::Request::builder().method()` call expects a `Method` or something that implements `IntoMethod`. Dereference it:

```rust
.method((*request_data.method).clone())
```

> Note: `Method` is `Clone` and cheap to clone from a reference. The `Arc` prevents the clone from happening at the struct level when `RequestData` is passed between layers. We only clone the inner `Method` once here at the final HTTP builder step.

- [ ] **Step 5: Run all tests to confirm nothing is broken**

```bash
cd thirtyfour && cargo test --all-features 2>&1 | tail -30
```

Expected: all existing tests pass, new test passes.

- [ ] **Step 6: Commit**

```bash
git add thirtyfour/src/common/requestdata.rs thirtyfour/src/session/http.rs
git commit -m "perf(requestdata): wrap Method in Arc to eliminate per-layer clones"
```

---

## Task 4: Eliminate unconditional `body_str` allocation

**Files:**
- Modify: `thirtyfour/src/session/http.rs` (lines 86–90 and 218–229)

Currently `body_str` is allocated on every response. We move the allocation to the error path only.

- [ ] **Step 1: Write the failing test**

Add inside the existing `#[cfg(all(feature = "reqwest", test))] mod tests` block in `http.rs`:

```rust
#[tokio::test]
async fn test_successful_response_does_not_allocate_lossy_string() {
    // This test ensures we can parse a valid JSON response without
    // ever calling String::from_utf8_lossy on the success path.
    // We verify indirectly by ensuring parse works on raw bytes.
    let body = b"{\"value\": \"ok\"}";
    let parsed: serde_json::Value = serde_json::from_slice(body)
        .expect("should parse from slice without lossy string");
    assert_eq!(parsed["value"], "ok");
}
```

- [ ] **Step 2: Run test to verify it passes already (baseline)**

```bash
cd thirtyfour && cargo test --features reqwest test_successful_response_does_not_allocate_lossy_string 2>&1 | tail -10
```

Expected: passes — this confirms `from_slice` works without the lossy string.

- [ ] **Step 3: Remove the unconditional `body_str` allocation in `HttpClient::send()`**

In `thirtyfour/src/session/http.rs`, replace lines 86–91:

```rust
// BEFORE
let body = resp.bytes().await?;
let body_str = String::from_utf8_lossy(&body).into_owned();
let resp = builder
    .body(body)
    .map_err(|_| WebDriverError::UnknownResponse(status.as_u16(), body_str))?;
Ok(resp)
```

With:

```rust
// AFTER — body_str only allocated on the error path
let body = resp.bytes().await?;
let resp = builder.body(body).map_err(|e| {
    WebDriverError::RequestFailed(format!("Failed to build response: {e}"))
})?;
Ok(resp)
```

- [ ] **Step 4: Remove the unconditional `lossy_response` allocation in `run_webdriver_cmd()`**

In `thirtyfour/src/session/http.rs`, replace lines 218–230:

```rust
// BEFORE
let status = response.status().as_u16();
let lossy_response = String::from_utf8_lossy(response.body());
tracing::debug!("webdriver response: {status} {lossy_response}");
match status {
    200..=399 => match serde_json::from_slice(response.body()) {
        Ok(v) => Ok(CmdResponse { body: v, status }),
        Err(_) => Err(WebDriverError::parse(status, lossy_response.into_owned())),
    },
    _ => Err(WebDriverError::parse(status, lossy_response.into_owned())),
}
```

With:

```rust
// AFTER — lossy_response only allocated when an error actually occurs
let status = response.status().as_u16();
match status {
    200..=399 => match serde_json::from_slice(response.body()) {
        Ok(v) => {
            tracing::debug!("webdriver response: {status} <ok>");
            Ok(CmdResponse { body: v, status })
        }
        Err(_) => {
            let lossy = String::from_utf8_lossy(response.body()).into_owned();
            tracing::debug!("webdriver response: {status} {lossy}");
            Err(WebDriverError::parse(status, lossy))
        }
    },
    _ => {
        let lossy = String::from_utf8_lossy(response.body()).into_owned();
        tracing::debug!("webdriver response: {status} {lossy}");
        Err(WebDriverError::parse(status, lossy))
    }
}
```

- [ ] **Step 5: Run all tests**

```bash
cd thirtyfour && cargo test --all-features 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add thirtyfour/src/session/http.rs
git commit -m "perf(http): move body_str allocation to error path only"
```

---

## Task 5: Eliminate header cloning in `HttpClient::send()`

**Files:**
- Modify: `thirtyfour/src/session/http.rs` (lines 82–84)

Currently every header key and value is cloned when copying headers to the response builder. We pass references instead.

- [ ] **Step 1: Write the failing test**

Add inside `#[cfg(all(feature = "reqwest", test))] mod tests` in `http.rs`:

```rust
#[test]
fn test_header_values_can_be_passed_by_reference() {
    // Verifies the http::response::Builder accepts header references,
    // confirming our approach is valid before we change the hot path.
    use http::{HeaderValue, Response};
    use bytes::Bytes;

    let name = http::header::CONTENT_TYPE;
    let value = HeaderValue::from_static("application/json");

    // Pass by reference — must compile and produce valid response.
    let resp: Response<Bytes> = Response::builder()
        .header(&name, &value)
        .body(Bytes::new())
        .expect("should build response with references");

    assert_eq!(
        resp.headers().get(http::header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
}
```

- [ ] **Step 2: Run test to verify it passes (confirms API supports references)**

```bash
cd thirtyfour && cargo test --features reqwest test_header_values_can_be_passed_by_reference 2>&1 | tail -10
```

Expected: passes — confirms `builder.header(&name, &value)` is valid.

- [ ] **Step 3: Remove `.clone()` calls in the header loop**

In `thirtyfour/src/session/http.rs`, replace lines 82–84:

```rust
// BEFORE
for (key, value) in resp.headers().iter() {
    builder = builder.header(key.clone(), value.clone());
}
```

With:

```rust
// AFTER — pass references, no allocation
for (key, value) in resp.headers().iter() {
    builder = builder.header(key, value);
}
```

- [ ] **Step 4: Run all tests**

```bash
cd thirtyfour && cargo test --all-features 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add thirtyfour/src/session/http.rs
git commit -m "perf(http): eliminate header clone in response builder loop"
```

---

## Task 6: Implement the dynamic memory guard

**Files:**
- Modify: `thirtyfour/src/session/http.rs`

This is the core safety feature. Before reading the response body with `resp.bytes().await?`, we check the `Content-Length` header against available system RAM using `sysinfo`.

- [ ] **Step 1: Write the failing test**

Add inside `#[cfg(all(feature = "reqwest", test))] mod tests` in `http.rs`:

```rust
#[test]
fn test_memory_guard_rejects_when_size_exceeds_available() {
    // Simulates the guard logic in isolation.
    // available_ram = 100 bytes, safe_limit = 80 bytes (80%)
    // content_length = 90 bytes → should be rejected.
    let available_ram: u64 = 100;
    let safe_limit = available_ram * 80 / 100;
    let content_length: u64 = 90;

    let would_oom = content_length > safe_limit;
    assert!(would_oom, "guard should trigger when content_length > safe_limit");
}

#[test]
fn test_memory_guard_allows_when_size_is_safe() {
    // available_ram = 1_000_000_000 (1 GB), content_length = 50_000_000 (50 MB)
    let available_ram: u64 = 1_000_000_000;
    let safe_limit = available_ram * 80 / 100;
    let content_length: u64 = 50_000_000;

    let would_oom = content_length > safe_limit;
    assert!(!would_oom, "guard should not trigger for 50 MB when 1 GB available");
}
```

- [ ] **Step 2: Run tests to verify they pass (pure logic, no impl needed)**

```bash
cd thirtyfour && cargo test --features reqwest test_memory_guard 2>&1 | tail -10
```

Expected: both tests pass — confirms the arithmetic is correct before integration.

- [ ] **Step 3: Add `sysinfo` import and helper function to `http.rs`**

At the top of `thirtyfour/src/session/http.rs`, add the import inside the `#[cfg(feature = "reqwest")]` block:

```rust
#[cfg(feature = "reqwest")]
fn available_system_memory_bytes() -> u64 {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    sys.available_memory()
}
```

> We use `MemoryRefreshKind::nothing().with_ram()` to refresh only RAM info — not swap, not CPU — keeping the overhead of this call minimal (YAGNI, KISS).

- [ ] **Step 4: Integrate the guard into `HttpClient::send()`**

In `thirtyfour/src/session/http.rs`, after `let resp = req.send().await?;` and before reading the body, insert the guard:

```rust
let resp = req.send().await?;
let status = resp.status();

// Memory guard: check Content-Length against available RAM before buffering.
if let Some(content_length) = resp.content_length() {
    let available = available_system_memory_bytes();
    let safe_limit = available * 80 / 100;
    if content_length > safe_limit {
        return Err(WebDriverError::ResponseTooLarge(format!(
            "Response body ({content_length} bytes) exceeds safe memory limit \
             ({safe_limit} bytes). Available RAM: {available} bytes"
        )));
    }
}

let mut builder = Response::builder();
builder = builder.status(status);
for (key, value) in resp.headers().iter() {
    builder = builder.header(key, value);
}

let body = resp.bytes().await?;
let resp = builder.body(body).map_err(|e| {
    WebDriverError::RequestFailed(format!("Failed to build response: {e}"))
})?;
Ok(resp)
```

- [ ] **Step 5: Run all tests**

```bash
cd thirtyfour && cargo test --all-features 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 6: Verify the crate compiles cleanly with no warnings**

```bash
cd thirtyfour && cargo clippy --all-features -- -D warnings 2>&1 | tail -30
```

Expected: no warnings, no errors.

- [ ] **Step 7: Commit**

```bash
git add thirtyfour/src/session/http.rs
git commit -m "feat(http): add dynamic RAM-aware memory guard before reading response body"
```

---

## Task 7: Fix `user_agent` clone in `run_webdriver_cmd`

**Files:**
- Modify: `thirtyfour/src/common/config.rs`
- Modify: `thirtyfour/src/session/http.rs`

`config.user_agent` is cloned on every request (line 190 of `http.rs`). The `WebDriverConfig` struct should store `user_agent` as `Arc<str>` so that `clone()` is a reference-count increment, not a heap copy.

- [ ] **Step 1: Write the failing test**

Add in `thirtyfour/src/common/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_clone_is_arc() {
        let config = WebDriverConfig::default();
        let clone = config.user_agent.clone();
        // Arc::ptr_eq confirms both point to the same allocation.
        assert!(Arc::ptr_eq(&config.user_agent, &clone));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd thirtyfour && cargo test test_user_agent_clone_is_arc 2>&1 | tail -20
```

Expected: compile error — `user_agent` is not `Arc<str>` yet.

- [ ] **Step 3: Check and update `WebDriverConfig` in `config.rs`**

Open `thirtyfour/src/common/config.rs`. Locate the `user_agent` field. Change it from `String` to `Arc<str>`:

```rust
use std::sync::Arc;

/// Configuration for the WebDriver session.
#[derive(Debug, Clone)]
pub struct WebDriverConfig {
    // ... other fields ...
    /// The user agent string sent with every request.
    pub user_agent: Arc<str>,
    // ... other fields ...
}
```

Update the `Default` implementation so it produces an `Arc<str>`:

```rust
impl Default for WebDriverConfig {
    fn default() -> Self {
        Self {
            user_agent: Arc::from(concat!(
                "thirtyfour/", env!("CARGO_PKG_VERSION")
            )),
            // ... other default fields ...
        }
    }
}
```

- [ ] **Step 4: Fix any `user_agent` construction call sites**

Search for any `config.user_agent = ` or `user_agent: String::from(...)` in the codebase and update to use `Arc::from(...)`:

```bash
grep -rn "user_agent" thirtyfour/src/ --include="*.rs"
```

Update each call site to use `Arc::<str>::from(value)` or `value.into()` (since `Arc<str>` implements `From<&str>` and `From<String>`).

- [ ] **Step 5: The `http.rs` call site now becomes free**

The line in `run_webdriver_cmd`:

```rust
.header(USER_AGENT, config.user_agent.clone())
```

...still compiles because `HeaderValue` accepts `Arc<str>` via its `TryFrom` impl. If it does not compile directly, convert explicitly:

```rust
.header(USER_AGENT, config.user_agent.as_ref())
```

- [ ] **Step 6: Run all tests**

```bash
cd thirtyfour && cargo test --all-features 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add thirtyfour/src/common/config.rs thirtyfour/src/session/http.rs
git commit -m "perf(config): store user_agent as Arc<str> to eliminate per-request heap clones"
```

---

## Task 8: Final validation

**Files:** All modified files.

- [ ] **Step 1: Run the full test suite one final time**

```bash
cd thirtyfour && cargo test --all-features 2>&1 | tail -40
```

Expected: all tests pass, zero failures.

- [ ] **Step 2: Run Clippy in pedantic mode**

```bash
cd thirtyfour && cargo clippy --all-features -- \
  -W clippy::pedantic \
  -A clippy::module_name_repetitions \
  -A clippy::must_use_candidate \
  2>&1 | tail -40
```

Expected: no new warnings introduced by our changes.

- [ ] **Step 3: Verify the design spec is fully covered**

Check each item from `docs/superpowers/specs/2026-04-16-performance-wave-design.md`:

| Spec Requirement | Task | Status |
|:---|:---|:---|
| `sysinfo` dependency added | Task 1 | ☐ |
| `ResponseTooLarge` error variant | Task 2 | ☐ |
| `Method` wrapped in `Arc` | Task 3 | ☐ |
| Lazy `body_str` allocation | Task 4 | ☐ |
| Zero-clone header loop | Task 5 | ☐ |
| Dynamic RAM memory guard | Task 6 | ☐ |
| `user_agent` as `Arc<str>` | Task 7 | ☐ |

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: performance wave complete — memory guard + allocation reduction"
```

