use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use forge_domain::Conversation;
use tokio::sync::Mutex;

use super::Result;
use crate::EventKind;

#[derive(Clone)]
pub struct Tracker {
    model: Arc<Mutex<Option<String>>>,
    conversation: Arc<Mutex<Option<Conversation>>>,
    is_logged_in: Arc<AtomicBool>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            model: Arc::new(Mutex::new(None)),
            conversation: Arc::new(Mutex::new(None)),
            is_logged_in: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Tracker {
    /// Sets the current model name for tracking purposes.
    pub async fn set_model<S: Into<String>>(&'static self, model: S) {
        let mut guard = self.model.lock().await;
        *guard = Some(model.into());
    }

    /// Records a login event.
    pub async fn login<S: Into<String>>(&'static self, login: S) {
        let is_logged_in = self.is_logged_in.load(Ordering::SeqCst);
        if is_logged_in {
            return;
        }
        self.is_logged_in.store(true, Ordering::SeqCst);
        let login_value = login.into();
        let id = crate::event::Identity { login: login_value };
        self.dispatch(EventKind::Login(id)).await.ok();
    }

    /// Dispatches an event.
    ///
    /// Note: All 3rd party telemetry has been removed. This method is a no-op.
    /// Local logging is handled separately via tracing.
    pub async fn dispatch(&self, _event_kind: EventKind) -> Result<()> {
        Ok(())
    }

    /// Sets the current conversation for tracking purposes.
    pub async fn set_conversation(&self, conversation: Conversation) {
        *self.conversation.lock().await = Some(conversation);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;

    static TRACKER: LazyLock<Tracker> = LazyLock::new(Tracker::default);

    #[tokio::test]
    async fn test_tracker_dispatch_is_noop() {
        let result = TRACKER
            .dispatch(EventKind::Prompt("ping".to_string()))
            .await;
        assert!(result.is_ok());
    }
}
