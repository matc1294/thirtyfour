# Performance & Efficiency Wave — Design Document

**Date:** 2026-04-16  
**Wave:** Option B (Performance & Efficiency)  
**Approach:** The "Dynamic Memory-Aware Guard" Refactor  

---

## Overview

This document describes the design for the **Performance & Efficiency refactoring wave** of the `thirtyfour` library. The goal is to eliminate redundant memory allocations in the HTTP hot path while providing production-grade protection against Out-Of-Memory (OOM) crashes caused by malicious or misbehaving WebDriver servers.

---

## Goals

1. Eliminate constant string and header cloning on every request.
2. Prevent OOM crashes via dynamic, system-aware response body limits.
3. Zero breaking changes to the public API.
4. Maintain full compatibility with Selenium JSON Wire Protocol.

---

## Design Decisions

### 1. Arc<str> for RequestData Fields

**Decision:** Change `RequestData` fields (`uri`, `method`) from owned types to `Arc`-backed shared references.

**Rationale:**
- Rust's `str` is a Dynamically Sized Type (DST) and can only exist behind a pointer.
- `Arc<str>` provides single indirection (Arc Pointer → Data Buffer).
- This eliminates the double-indirection of `Arc<String>` (Arc Pointer → String Struct → Data Buffer), reducing memory overhead.
- In async contexts, `&str` references cannot survive `.await` points without an owned backing. `Arc<str>` ensures data lives as long as needed across async boundaries.

**Implementation:**
```rust
#[derive(Clone)]
pub struct RequestData {
    pub uri: Arc<str>,      // Changed from String
    pub method: Arc<Method>,// Changed from Method (owned)
    pub body: Option<Value>,
}
```

### 2. Dynamic Memory-Aware Response Guard

**Decision:** Implement a runtime check in `HttpClient::send()` that compares the response `Content-Length` against actual system RAM availability before reading the body.

**Rationale:**
- No hardcoded thresholds — the limit is derived from real machine state.
- Cross-platform support via the `sysinfo` crate (`System::available_memory()`).
- Fails gracefully with a clear error message instead of crashing the process.

**Implementation (in `thirtyfour/src/session/http.rs`):**
```rust
async fn send(&self, request: Request<Body<'_>>) -> WebDriverResult<Response<Bytes>> {
    let resp = req.send().await?;

    // Step 1: Read headers BEFORE reading body.
    let content_length = resp.content_length(); // Option<u64>

    // Step 2: Get real system memory availability.
    let mut sys = System::new_with_specifics(RefreshKind::nothing().with_memory());
    sys.refresh_memory();
    let available_ram = sys.available_memory();

    // Step 3: Dynamic risk assessment.
    if let Some(size) = content_length {
        // Keep 20% RAM headroom for OS and safety margins.
        let safe_limit = available_ram * 80 / 100;
        if size > safe_limit {
            return Err(WebDriverError::OOM(format!(
                "Response body ({} bytes) exceeds safe memory limit ({} bytes). \
                 Available RAM: {} bytes",
                size, safe_limit, available_ram
            )));
        }
    }

    // Step 4: Safe to proceed — buffer and process.
    let body = resp.bytes().await?;
    Ok(builder.body(body).map_err(|_| ...)?)
}
```

### 3. Conditional Error Body Allocation

**Decision:** Only convert the response body into an owned `String` if a WebDriver error occurs.

**Rationale:**
- The current implementation calls `.into_owned()` on every response to create `body_str`, even for successful requests.
- This creates massive unnecessary memory pressure (especially for large payloads like 50MB Base64 images).
- Moving this allocation to the error path eliminates the overhead from all happy-path responses.

**Implementation:**
```rust
// Before: Always allocated, never used in success path.
let body = resp.bytes().await?;
let body_str = String::from_utf8_lossy(&body).into_owned(); // ← REMOVE THIS

// After: Allocate only on error.
match serde_json::from_slice(response.body()) {
    Ok(v) => Ok(CmdResponse { body: v, status }),
    Err(_) => {
        let lossy_response = String::from_utf8_lossy(response.body()).into_owned();
        Err(WebDriverError::parse(status, lossy_response))
    }
}
```

### 4. Zero-Clone Header Processing

**Decision:** Iterate over response headers using references instead of cloning every key and value.

**Rationale:**
- The current loop clones all headers on every request (`value.clone()`).
- Headers are read-only metadata — no mutation is needed.
- Passing `&key` and `&value` to the builder eliminates these allocations.

**Implementation:**
```rust
// Before:
for (key, value) in resp.headers().iter() {
    req = req.header(key.clone(), value.clone()); // ← Clones every header
}

// After:
for (key, value) in resp.headers().iter() {
    req = req.header(key, value); // ← Pass by reference
}
```

---

## No Breaking Changes

All changes are internal to `thirtyfour/src/session/http.rs` and `thirtyfour/src/common/request.rs`. The public API remains unchanged:

- `WebDriver::click()`, `.execute()`, etc. — identical behavior.
- `CmdResponse::value()` — returns the same type (`Value`), same error semantics.
- If a response is too large for available memory, users receive a clear `WebDriverError::OOM` with details about the limit and actual RAM.

---

## New Dependency

Add to `Cargo.toml`:

```toml
[dependencies]
sysinfo = "0.32"  # Cross-platform system information (memory, CPU)
```

The dependency is small (~200KB) and does not add significant compile-time overhead.

---

## Testing Strategy

1. **Unit tests** for the memory guard logic with mocked `System` behavior.
2. **Integration tests** simulating large payloads to verify OOM protection triggers correctly on low-memory systems (via cgroup/container memory limits).
3. **Benchmark tests** comparing allocation counts before/after refactoring using `dhat` or similar heap profiling tools.

---

## Files Affected

| File | Changes |
|:---|:---|
| `thirtyfour/Cargo.toml` | Add `sysinfo` dependency |
| `thirtyfour/src/common/request.rs` | Change fields to `Arc<str>` and `Arc<Method>` |
| `thirtyfour/src/session/http.rs` | Implement memory guard, conditional allocation, reference-based headers |

---

## Future Considerations

- **Streaming API:** If users consistently need payloads larger than available RAM (e.g., downloading multi-GB files), a future wave may introduce streaming variants like `.execute_stream()`.
- **Caching System Memory:** To avoid calling `System::refresh_memory()` on every single request, we can cache the value and refresh it periodically (e.g., once per second) via an internal timer or lazy cell.

---

## Status

✅ Design Approved — Ready for Implementation Planning
