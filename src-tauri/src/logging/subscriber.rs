//! Subscriber composition — Filter Layer + Fanout Layer (spec §2.2).
//!
//! The composition is exposed via `build()` and consumed by `logging::init()`
//! in Task 6.

use crate::logging::fanout::{FanoutLayer, FanoutLayerHandle};
use crate::logging::filter_layer;
use crate::logging::event::LoggedEvent;
use crate::session_log::SessionLogState;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, reload::Handle, Registry};

pub struct SubscriberHandles {
    pub filter_reload: Handle<EnvFilter, Registry>,
    pub fanout: FanoutLayerHandle,
    pub broadcast_rx: broadcast::Receiver<LoggedEvent>,
}

pub fn build(session_log: Arc<SessionLogState>) -> (impl tracing::Subscriber + Send + Sync, SubscriberHandles) {
    let (filter, filter_reload) = filter_layer::build();
    let (fanout, broadcast_rx) = FanoutLayer::create(session_log);

    let subscriber = Registry::default()
        .with(filter)
        .with(fanout.clone());

    let handles = SubscriberHandles {
        filter_reload,
        fanout,
        broadcast_rx,
    };
    (subscriber, handles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_log::SessionLogState;

    #[test]
    fn build_returns_subscriber_and_handles() {
        let session_log = Arc::new(SessionLogState::new(100));
        let (_sub, _handles) = build(session_log);
    }
}
