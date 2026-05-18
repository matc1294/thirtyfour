# AGENTS.md

## Code Review Experts

The following experts are available for code review of the `thirtyfour` library:

### Rust Expert
- **Domain:** Idiomatic Rust, memory safety, async/await patterns, and performance.
- **File Patterns:** `*.rs`
- **Key Checks:** 
  - Clippy pedantic warnings
  - Proper error handling (avoiding `.unwrap()`, using `thiserror`/`anyhow`)
  - Efficient use of smart pointers (`Arc`, `Box`, `Rc`)
  - Async cancellation safety and deadlock prevention

### Architecture & Clean Code Expert
- **Domain:** Design patterns, SOLID, KISS, DRY, YAGNI, and API ergonomics.
- **File Patterns:** `*.rs` (whole project)
- **Key Checks:** 
  - Violation of SOLID principles (Single Responsibility, Open/Closed, etc.)
  - Over-engineering (YAGNI)
  - Code duplication (DRY)
  - Complexity vs Simplicity (KISS)
  - API consistency and ergonomics for the end-user

### Security Expert
- **Domain:** Secure coding practices, vulnerability detection, and network security.
- **File Patterns:** `*.rs`
- **Key Checks:** 
  - Proper handling of sensitive data
  - Potential injection points in WebDriver commands
  - TLS/SSL configuration and validation
  - Resource exhaustion (DoS) vulnerabilities

### Performance Expert
- **Domain:** Execution efficiency, network latency optimization, and resource usage.
- **File Patterns:** `*.rs`
- **Key Checks:** 
  - Unnecessary allocations or clones in hot paths
  - Inefficient async synchronization primitives
  - Network request overhead (too many round trips)
  - Algorithmic complexity of core logic

### Documentation Expert
- **Domain:** API documentation, clarity, and completeness.
- **File Patterns:** `*.rs`, `docs/**/*.md`
- **Key Checks:** 
  - Missing doc comments on public items (`///`)
  - Outdated or misleading documentation
  - Lack of usage examples for complex functions
  - Inconsistency between code behavior and documented intent
