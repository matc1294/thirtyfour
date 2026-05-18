# Pending Tasks from Code Review

**Review Date:** 2026-04-16
**Branch:** main (Audit)

## 🔴 Critical (Must Fix Immediately)
- [ ] **C1: Prevent OOM DoS** - Implement response body size limits in `thirtyfour/src/session/http.rs`
- [ ] **C2: Refactor Command God Object** - Replace the `Command` enum with a trait-based system to satisfy OCP (`thirtyfour/src/common/command.rs`)
- [ ] **C3: Optimize Response Allocation** - Remove unconditional `.into_owned()` on response bodies in `thirtyfour/src/session/http.rs`
- [ ] **C4: Document BiDi** - Create a comprehensive user guide for WebDriver BiDi extensions in `docs/src`

## 🟠 High Priority (Performance & Ergonomics)
- [ ] **H1: Fix Deref Misuse** - Remove inheritance-style `Deref` on `WebDriver` (`thirtyfour/src/web_driver.rs`)
- [ ] **H2: Eliminate Config Cloning** - Change User-Agent and other static configs to `Arc<str>` (`thirtyfour/src/session/http.rs`)
- [ ] **H3: Reduce Header Churn** - Optimize header processing in the HTTP client loop (`thirtyfour/src/session/http.rs`)
- [ ] **H4: Doc Core Types** - Add `///` documentation to `ActionChain`, `Alert`, and other core types

## 🟡 Medium Priority (Security & Design)
- [ ] **M1: Secure Auth Headers** - Mark `Authorization` headers as sensitive (`thirtyfour/src/session/http.rs`)
- [ ] **M2: Remove Redundant URI Strings** - Pass URIs directly to the request builder (`thirtyfour/src/session/http.rs`)
- [ ] **M3: Split SessionHandle** - Separate state management from transport logic (`thirtyfour/src/session/handle.rs`)
- [ ] **M4: Enrich API Examples** - Add usage examples to `ActionChain` and `WebElement` methods

## 🔵 Low Priority (Polish)
- [ ] **L1: Sanitize Debug Logs** - Mask sensitive data in `tracing::debug!` calls (`thirtyfour/src/session/http.rs`)
- [ ] **L2: Optimize Method Cloning** - Remove redundant method clones in the request loop
- [ ] **L3: Reduce Request Boilerplate** - Implement a more fluent API for executing commands

## Progress Tracking
Update this file as you complete tasks. Mark with [x] and add commit hash.
