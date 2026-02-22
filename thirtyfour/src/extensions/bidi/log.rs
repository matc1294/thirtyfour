use super::{BiDiEvent, BiDiSession};
use serde::Deserialize;
use tokio::sync::broadcast;

/// A single log entry received from the browser.
#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    /// The log level (e.g., "info", "warning", "error").
    pub level: String,
    /// The log message text.
    pub text: Option<String>,
    /// Unix timestamp in milliseconds.
    pub timestamp: Option<u64>,
    /// Source information for the log entry.
    pub source: Option<LogSource>,
    /// Additional fields not covered by this struct.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Source information for a log entry.
#[derive(Debug, Clone, Deserialize)]
pub struct LogSource {
    /// The realm where the log entry originated.
    pub realm: Option<String>,
    /// The browsing context where the log entry originated.
    pub context: Option<String>,
}

/// Log domain events.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LogEvent {
    /// A log entry was added.
    EntryAdded(LogEntry),
}

/// Console domain event (re-exported from log).
#[derive(Debug, Clone)]
pub struct ConsoleEvent(pub LogEntry);

/// `BiDi` `log` domain accessor.
#[derive(Debug)]
pub struct Log<'a> {
    session: &'a BiDiSession,
}

impl<'a> Log<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Subscribe to `log.entryAdded` events. Returns a broadcast receiver.
    /// You must first call `bidi.session().subscribe(&["log.entryAdded"], &[]).await?`
    /// to tell the browser to start sending these events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<BiDiEvent> {
        self.session.subscribe_events()
    }
}
