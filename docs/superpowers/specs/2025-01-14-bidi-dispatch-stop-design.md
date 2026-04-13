# BiDi Session Dispatch Stop/Cancel Mechanism

**Date:** 2025-01-14
**Status:** Approved Design
**Author:** Mux

## Overview

This design adds graceful shutdown and immediate cancellation capabilities to the WebDriver BiDi session's dispatch loop. Currently, when users spawn the dispatch task via `tokio::spawn(bidi.dispatch_future())`, there is no built-in mechanism to stop or cancel that task. The task runs indefinitely until the WebSocket connection closes naturally.

This design addresses three use cases:
1. **Explicit shutdown** - Stop the task when the application is shutting down or done using BiDi
2. **Error condition handling** - Stop the task in error conditions or timeout scenarios
3. **Restart capability** - Allow restarting the dispatch loop after graceful shutdown (but not after cancellation)

## Architecture

### Core Changes to `BiDiSession`

- Add internal fields to track the dispatch task handle and state
- Add `stop_dispatch()` method for graceful shutdown
- Add `cancel_dispatch()` method for immediate cancellation
- Modify `dispatch_future()` to support cancellation tokens

### State Management

**DispatchState Enum (simplified):**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchState {
    Idle,      // No dispatch loop started
    Running,   // Dispatch loop is active
    Stopped,   // Dispatch loop stopped gracefully (restartable if connected)
    Cancelled, // Dispatch loop cancelled immediately (not restartable)
}
```

**New Fields:**
- `dispatch_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>` - Stores spawned task handle
- `dispatch_state: Arc<AtomicU8>` - Tracks current state (using atomic u8 to avoid extra dependencies)
- `cancel_token: CancellationToken` - Single token for cancellation capability

### Key Behaviors

- After calling `dispatch_future()`, state becomes `Running` and handle is stored
- `stop_dispatch()` sends WebSocket close frame, awaits JoinHandle, transitions to `Stopped`
- `cancel_dispatch()` cancels token, aborts task, transitions to `Cancelled`
- If state is `Stopped` and connection is open, `dispatch_future()` can be called again (restart)
- If state is `Cancelled`, `dispatch_future()` returns `None` (no restart allowed)
- Methods handle edge cases: already stopped, task finished, no task started, etc.

## API Design

### New Methods

#### `stop_shutdown()`

Gracefully stop the dispatch loop by sending a WebSocket close frame and waiting for the task to complete.

```rust
pub async fn stop_dispatch(&self, timeout: Duration) -> WebDriverResult<()>
```

**Errors:**
- `BiDiDispatchNotStarted` - No dispatch loop is currently running
- `BiDiDispatchTimeout` - Task did not complete within timeout

#### `cancel_dispatch()`

Immediately cancel the dispatch loop using a cancellation token. The task is aborted without waiting for graceful shutdown. After cancellation, the session cannot be restarted.

```rust
pub async fn cancel_dispatch(&self) -> WebDriverResult<()>
```

**Errors:**
- `BiDiDispatchNotStarted` - No dispatch loop is currently running



### Optional Convenience Method

#### `spawn_dispatch()`

Convenience method to spawn the dispatch loop and store the handle internally.

```rust
pub async fn spawn_dispatch(&mut self) -> WebDriverResult<()>
```

This allows users to write:
```rust
bidi.spawn_dispatch().await?;
// later...
bidi.stop_dispatch(Duration::from_secs(5)).await?;
```

Instead of:
```rust
let handle = tokio::spawn(bidi.dispatch_future()?);
// later... (no way to stop)
```

### Modified Types

#### `DispatchFuture`

Add cancellation token field:
```rust
pub struct DispatchFuture {
    stream: /* existing */,
    ctx: /* existing */,
    #[pin]
    span: /* existing */,
    cancel_token: CancellationToken, // NEW
}
```

### New Error Variants

Add to `WebDriverError`:
```rust
BiDiDispatchNotStarted, // No dispatch loop running
BiDiDispatchTimeout,    // Graceful shutdown timed out
```

Note: `AlreadyStopped` and `RestartFailed` cases can be handled with `BiDiDispatchNotStarted` for simplicity.

## Data Flow

### Graceful Shutdown Flow

1. Check current state: if not `Running`, return `BiDiDispatchNotStarted` error
2. Transition state to `Stopped`
3. Send WebSocket close frame via `ws_sink.lock().await.send(Message::Close(None)).await`
4. Take the `JoinHandle` from `dispatch_handle` (set to `None`)
5. Call helper `cleanup_dispatch_task(handle, timeout)`:
   - `tokio::time::timeout(timeout, handle).await` - wait for task to complete
   - On timeout, force-kill with `handle.abort()`, return `BiDiDispatchTimeout` error
   - On success, return `Ok(())`
6. Update `connected` flag to `false` via helper

### Immediate Cancellation Flow

1. Check current state: if not `Running`, return `BiDiDispatchNotStarted` error
2. Transition state to `Cancelled`
3. Call `cancel_token.cancel()`
4. Take the `JoinHandle` from `dispatch_handle` (set to `None`)
5. Call helper `cleanup_dispatch_task(handle, Duration::from_secs(1))`:
   - `handle.await` - task should exit quickly due to cancelled token
   - Short timeout (1s) to ensure cleanup doesn't hang
6. Update `connected` flag to `false` via helper

### Restart Flow (via `dispatch_future()` after stop)

1. Check current state:
   - If `Idle` or `Stopped` and `connected == true`: allow restart
   - If `Running`: return `None` (already running, user should stop first)
   - If `Cancelled`: return `None` (cannot restart after cancellation)
2. Transition state to `Running`
3. Return `DispatchFuture` with `cancel_token` cloned

### Helper Functions (DRY - Shared Logic)

```rust
async fn cleanup_dispatch_task(
    handle: JoinHandle<()>,
    timeout: Duration,
) -> WebDriverResult<()> {
    match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            tracing::warn!("Dispatch task panicked: {}", e);
            Ok(()) // Task finished, even if panicked
        }
        Err(_) => {
            handle.abort();
            Err(WebDriverError::BiDiDispatchTimeout)
        }
    }
}

fn mark_disconnected(&self) {
    self.connected.store(false, Ordering::Relaxed);
    let _ = self.event_tx.send(BiDiEvent::ConnectionClosed);
}
```

### DispatchFuture Polling Changes

In `Future::poll()` implementation:
- At start of each poll iteration, check `cancel_token.is_cancelled()`
- If cancelled, set `connected` to `false`, send `ConnectionClosed` event, return `Poll::Ready(())`
- Allows clean exit even during cancellation

## Edge Case Handling

| Scenario | Handling |
|----------|----------|
| Task already finished when stop/cancel called | Treat as success - transition to appropriate state and return |
| Double stop/cancel | Return error - state is already `Stopped` or `Cancelled` |
| No task started | Return `BiDiDispatchNotStarted` error |
| WebSocket close frame fails | Log warning but continue - task will exit when stream ends |
| Timeout on graceful stop | Force abort with `handle.abort()` and return timeout error |
| WebSocket already closed | Continue with task cleanup - no need to send close frame |

## Testing Strategy

### Unit Tests

1. **State Transitions:**
   - `Idle` → `Running` when `dispatch_future()` is called
   - `Running` → `Stopping` → `Stopped` when `stop_dispatch()` is called
   - `Running` → `Cancelled` when `cancel_dispatch()` is called
   - `Stopped` → `Running` allows restart
   - `Cancelled` → `Running` fails

2. **Edge Cases:**
   - Calling `stop_dispatch()` twice (second should error)
   - Calling `cancel_dispatch()` twice (second should error)
   - Calling `stop_dispatch()` when no task running
   - Calling methods after WebSocket naturally closes

3. **Timeout Behavior:**
   - Mock task that sleeps longer than timeout
   - Verify `stop_dispatch()` aborts after timeout and returns timeout error
   - Verify `connection` flag is set to false even after timeout abort

### Integration Tests

1. **Graceful Shutdown End-to-End:**
   - Create real BiDi session with test driver
   - Spawn dispatch task
   - Send BiDi commands to verify it's working
   - Call `stop_dispatch()`
   - Verify task completes gracefully
   - Verify no commands can be sent after stop

2. **Immediate Cancellation End-to-End:**
   - Create real BiDi session
   - Spawn dispatch task
   - Call `cancel_dispatch()` immediately
   - Verify task exits quickly (<100ms)
   - Verify restart is not allowed

3. **Restart After Graceful Stop:**
   - Create BiDi session, spawn dispatch, call `stop_dispatch()`
   - Call `dispatch_future()` again and spawn
   - Verify dispatch loop is running again
   - Verify commands still work

### Test Infrastructure

- Use mock WebSocket server for controlled testing
- Use `tokio::time::pause()` for testing timeout behavior without real delays
- Use `tokio::test` macro for async tests

## Dependencies

**New dependency:**
- `tokio-util` - For `CancellationToken` (lightweight, well-maintained, commonly used with tokio)

Add to `Cargo.toml`:
```toml
[dependencies]
tokio-util = { version = "0.7", features = ["sync"], optional = true }
```

## Backward Compatibility

This design maintains full backward compatibility:
- Existing code that uses `tokio::spawn(bidi.dispatch_future())` continues to work unchanged
- New methods are opt-in
- No changes to existing public method signatures (except optional `spawn_dispatch()` convenience method)

## Implementation Notes

1. **State Tracking:** Use `Arc<AtomicCell<DispatchState>>` for thread-safe state tracking without locks in the hot path. Alternative: `Arc<AtomicU8>` with bit flags if minimizing dependencies.

2. **Handle Storage:** The `dispatch_handle` needs to be stored. This requires users to either:
   - Use the new `spawn_dispatch()` convenience method
   - Manually register the handle after spawning (we could add a `register_dispatch_handle()` method)
   - Design choice: add `spawn_dispatch()` as the primary API, keep `dispatch_future()` for advanced use cases

3. **Token Lifecycle:** Parent token is created when `BiDiSession` is created. Child token is created in `dispatch_future()` and passed to `DispatchFuture`. Both tokens are cancelled in `cancel_dispatch()`.

4. **Thread Safety:** All new fields use `Arc` with appropriate synchronization primitives (`TokioMutex`, `AtomicCell`, `CancellationToken` is thread-safe).

## Open Questions

1. Should `dispatch_future()` automatically register the handle when spawned, or should users call a separate `register_dispatch_handle()` method?
   - **Decision:** Provide `spawn_dispatch()` convenience method for automatic registration. Keep `dispatch_future()` for manual control.

2. Should the timeout for `stop_dispatch()` have a default value?
   - **Decision:** Require explicit timeout parameter to avoid implicit behavior.

3. Should `stop_dispatch()` and `cancel_dispatch()` return the task's result, or just indicate success/failure?
   - **Decision:** Return `WebDriverResult<()>` - task result is not useful to users, success/failure is what matters.
