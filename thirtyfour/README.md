[![Crates.io](https://img.shields.io/crates/v/thirtyfour.svg)](https://crates.io/crates/thirtyfour)
[![docs.rs](https://docs.rs/thirtyfour/badge.svg)](https://docs.rs/thirtyfour)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Vrtgs/thirtyfour/test.yml?branch=main)](https://github.com/Vrtgs/thirtyfour/actions)
[![code coverage](https://codecov.io/github/Vrtgs/thirtyfour/graph/badge.svg?token=Z3GDO1EXCX)](https://codecov.io/github/Vrtgs/thirtyfour)

# thirtyfour

Thirtyfour is a Selenium / WebDriver library for Rust, for automated website UI testing.


## Features

- All W3C WebDriver and WebElement methods supported
- Create new browser session directly via WebDriver (e.g. chromedriver)
- Create new browser session via Selenium Standalone or Grid
- Find elements (via all common selectors e.g. Id, Class, CSS, Tag, XPath)
- Send keys to elements, including key-combinations
- Execute Javascript
- Action Chains
- Get and set cookies
- Switch to frame/window/element/alert
- Shadow DOM support
- Alert support
- Capture / Save screenshot of browser or individual element as PNG
- Chrome DevTools Protocol (CDP) support (limited)
- Advanced query interface including explicit waits and various predicates
- Component Wrappers (similar to `Page Object Model`)

## Feature Flags

- `rustls`: (Default) Use rustls to provide TLS support (via reqwest).
- `native-tls`: Use native TLS (via reqwest).
- `component`: (Default) Enable the `Component` derive macro (via thirtyfour_macros).
- `bidi`: Enable WebDriver BiDi (bidirectional protocol) support for real-time
  event handling, network interception, script evaluation, and more.
  Requires Chrome 115+ or Firefox 119+. See `examples/bidi_network_intercept.rs`
  for usage examples.

### Example (async):

Requires a running WebDriver server (e.g., `chromedriver --port=9515`).

```rust
use thirtyfour::prelude::*;

#[tokio::main]
async fn main() -> WebDriverResult<()> {
     let caps = DesiredCapabilities::chrome();
     let driver = WebDriver::new("http://localhost:9515", caps).await?;

     // Navigate to https://wikipedia.org.
     driver.goto("https://wikipedia.org").await?;
     let elem_form = driver.find(By::Id("search-form")).await?;

     // Find element from element.
     let elem_text = elem_form.find(By::Id("searchInput")).await?;

     // Type in the search terms.
     elem_text.send_keys("selenium").await?;

     // Click the search button.
     let elem_button = elem_form.find(By::Css("button[type='submit']")).await?;
     elem_button.click().await?;

     // Look for header to implicitly wait for the page to load.
     driver.find(By::ClassName("firstHeading")).await?;
     assert_eq!(driver.title().await?, "Selenium - Wikipedia");

     // Always explicitly close the browser.
     driver.quit().await?;

     Ok(())
}
```

### BiDi Example (async):

Enable BiDi support in Chrome and intercept network requests:

```rust
use thirtyfour::extensions::bidi::{network::InterceptPhase, BiDiEvent, NetworkEvent};
use thirtyfour::prelude::*;

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let mut caps = DesiredCapabilities::chrome();
    // Enable BiDi protocol support
    caps.insert_base_capability("webSocketUrl".to_string(), serde_json::Value::Bool(true));

    let driver = WebDriver::new("http://localhost:9515", caps).await?;

    // Connect to the BiDi channel
    let mut bidi = driver.bidi_connect().await?;

    // Spawn the dispatch loop to process incoming WebSocket messages
    tokio::spawn(bidi.dispatch_future().expect("dispatch already started"));

    // Subscribe to network events
    let mut rx = bidi.subscribe_network().await?;

    driver.goto("https://example.com").await?;

    // Receive network events
    if let Ok(NetworkEvent::BeforeRequestSent(e)) = rx.recv().await {
        println!("Request: {} {}", e.request.method, e.request.url);
    }

    driver.quit().await?;
    Ok(())
}
```

## Minimum Supported Rust Version

The MSRV for `thirtyfour` is currently latest, and so its guaranteed to run the latest stable release.

## LICENSE

This work is dual-licensed under MIT or Apache 2.0.
You can choose either license if you use this work.

See the NOTICE file for more details.

`SPDX-License-Identifier: MIT OR Apache-2.0`
