pub use super::log::ConsoleEvent;
use super::{BiDiEvent, BiDiSession};
use tokio::sync::broadcast;

/// BiDi `console` domain accessor (thin wrapper over log.entryAdded events with console source).
#[derive(Debug)]
pub struct Console<'a> {
    session: &'a BiDiSession,
}

impl<'a> Console<'a> {
    pub(super) fn new(session: &'a BiDiSession) -> Self {
        Self {
            session,
        }
    }

    /// Subscribe to all BiDi events (caller should filter for log.entryAdded + console source).
    pub fn subscribe(&self) -> broadcast::Receiver<BiDiEvent> {
        self.session.subscribe_events()
    }
}
