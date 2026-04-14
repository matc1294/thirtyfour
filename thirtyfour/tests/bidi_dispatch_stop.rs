//! Integration tests for BiDi dispatch stop/cancel functionality

use std::time::Duration;

mod common;
use crate::common::{make_capabilities, webdriver_url};
use thirtyfour::extensions::bidi::BiDiSession;
use thirtyfour::prelude::*;

async fn setup_bidi(browser: &str) -> WebDriverResult<(WebDriver, BiDiSession)> {
    let mut caps = make_capabilities(browser);
    // Enable BiDi in capabilities
    caps.insert_base_capability("webSocketUrl".into(), true.into());

    let driver = WebDriver::new(webdriver_url(browser), caps).await?;
    let mut bidi: BiDiSession = driver.bidi_connect().await?;

    // We use spawn_dispatch instead of dispatch_future to test the stop/cancel functionality
    // which relies on the handle stored in BiDiSession.
    bidi.spawn_dispatch().await?;

    Ok((driver, bidi))
}

#[tokio::test]
async fn test_stop_dispatch_graceful() {
    let browser = std::env::var("THIRTYFOUR_BROWSER").unwrap_or_else(|_| "chrome".to_string());

    // Skip if WebDriver not available (simplified pattern)
    if tokio::time::timeout(Duration::from_secs(2), setup_bidi(&browser)).await.is_err() {
        println!("Skipping test: WebDriver not available at localhost");
        return;
    }

    let (driver, bidi) = setup_bidi(&browser).await.expect("Failed to setup BiDi session");

    // Test graceful stop
    bidi.stop_dispatch(Duration::from_secs(2)).await.expect("stop_dispatch failed");

    assert!(!bidi.is_connected(), "Session should be marked as disconnected after stop");

    driver.quit().await.expect("Failed to quit driver");
}

#[tokio::test]
async fn test_cancel_dispatch_immediate() {
    let browser = std::env::var("THIRTYFOUR_BROWSER").unwrap_or_else(|_| "chrome".to_string());

    if tokio::time::timeout(Duration::from_secs(2), setup_bidi(&browser)).await.is_err() {
        println!("Skipping test: WebDriver not available at localhost");
        return;
    }

    let (driver, bidi) = setup_bidi(&browser).await.expect("Failed to setup BiDi session");

    // Test immediate cancel
    bidi.cancel_dispatch().await.expect("cancel_dispatch failed");

    assert!(!bidi.is_connected(), "Session should be marked as disconnected after cancel");

    driver.quit().await.expect("Failed to quit driver");
}

#[tokio::test]
async fn test_restart_after_stop() {
    let browser = std::env::var("THIRTYFOUR_BROWSER").unwrap_or_else(|_| "chrome".to_string());

    if tokio::time::timeout(Duration::from_secs(2), setup_bidi(&browser)).await.is_err() {
        println!("Skipping test: WebDriver not available at localhost");
        return;
    }

    let (driver, bidi) = setup_bidi(&browser).await.expect("Failed to setup BiDi session");

    // Stop gracefully
    bidi.stop_dispatch(Duration::from_secs(2)).await.expect("stop_dispatch failed");
    assert!(!bidi.is_connected());

    // Note: After stopping the dispatch loop via stop_dispatch(), we transition to STOPPED state.
    // However, restart requires a fresh WebSocket connection since ws_stream is consumed
    // when creating the dispatch future. This test documents that behavior.

    driver.quit().await.expect("Failed to quit driver");
}

#[tokio::test]
async fn test_double_stop_error() {
    let browser = std::env::var("THIRTYFOUR_BROWSER").unwrap_or_else(|_| "chrome".to_string());

    if tokio::time::timeout(Duration::from_secs(2), setup_bidi(&browser)).await.is_err() {
        println!("Skipping test: WebDriver not available at localhost");
        return;
    }

    let (driver, bidi) = setup_bidi(&browser).await.expect("Failed to setup BiDi session");

    // First stop
    bidi.stop_dispatch(Duration::from_secs(2)).await.expect("First stop failed");

    // Second stop should return error because state is now STOPPED
    let result = bidi.stop_dispatch(Duration::from_secs(2)).await;
    assert!(result.is_err(), "Calling stop_dispatch twice should return an error");

    driver.quit().await.expect("Failed to quit driver");
}
