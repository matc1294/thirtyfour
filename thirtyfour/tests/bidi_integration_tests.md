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
    let bidi = BiDiSessionBuilder::new()
        .command_timeout(Duration::from_secs(10))
        .connect_with_driver(&driver)
        .await?;

    // Test network interception
    let mut rx = bidi.subscribe_network().await?;
    driver.goto("https://example.com").await?;

    // Wait for network event
    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("channel closed");

    println!("Received event: {:?}", event);

    driver.quit().await?;
    Ok(())
}
```

## Common Issues

### Chrome: webSocketUrl not returned

Ensure you set the capability:

```rust
caps.insert_base_capability("webSocketUrl".into(), true.into());
```

### Events not received

Make sure you call `session.subscribe()` before expecting events:

```rust
bidi.session().subscribe(&["network.*"], &[]).await?;
```

Or use the auto-subscribe convenience methods:

```rust
let rx = bidi.subscribe_network().await?;
```

### Timeout errors

Consider increasing the command timeout for slow operations:

```rust
let bidi = BiDiSessionBuilder::new()
    .command_timeout(Duration::from_secs(30))
    .connect_with_driver(&driver)
    .await?;
```
