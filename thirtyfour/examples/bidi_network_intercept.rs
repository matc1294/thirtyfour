//! Example: intercept network requests using WebDriver BiDi.
//!
//! Requires Chrome 115+ started with BiDi support:
//! ```json
//! { "webSocketUrl": true }
//! ```
//!
//! Run with:
//! ```
//! cargo run --example bidi_network_intercept --features bidi
//! ```

use thirtyfour::extensions::bidi::{network::InterceptPhase, BiDiEvent, NetworkEvent};
use thirtyfour::prelude::*;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    // Enable BiDi by requesting webSocketUrl in capabilities.
    let mut caps = DesiredCapabilities::chrome();
    caps.insert_base_capability("webSocketUrl".to_string(), serde_json::Value::Bool(true));

    let driver = WebDriver::new("http://localhost:4444", caps).await?;

    // Connect to the BiDi channel.
    let bidi = driver.bidi_connect().await?;

    // Tell the browser we want network events.
    bidi.session().subscribe(&["network.beforeRequestSent"], &[]).await?;

    // Add a network intercept for the beforeRequestSent phase.
    let intercept_id = bidi.network().add_intercept(&[InterceptPhase::BeforeRequestSent]).await?;

    // Start listening to events.
    let mut rx = bidi.subscribe_events();

    // Navigate — this triggers network events.
    driver.goto("https://example.com").await?;

    // Read a few events.
    for _ in 0..3 {
        match rx.recv().await {
            Ok(BiDiEvent::Network(NetworkEvent::BeforeRequestSent(e))) => {
                println!("Intercepted: {} {}", e.request.method, e.request.url);
                // Continue the request.
                bidi.network().continue_request(&e.request_id).await?;
            }
            Ok(event) => println!("Other event: {event:?}"),
            Err(e) => {
                println!("Recv error: {e}");
                break;
            }
        }
    }

    // Clean up.
    bidi.network().remove_intercept(&intercept_id).await?;
    driver.quit().await?;

    Ok(())
}
