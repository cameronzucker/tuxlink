//! Subscriber composition — Filter Layer + Fanout Layer (spec §2.2).
//!
//! The composition is exposed via `build()` and consumed by `logging::init()`
//! in Task 6.

use crate::logging::event::LoggedEvent;
use crate::logging::fanout::{FanoutLayer, FanoutLayerHandle};
use crate::logging::filter_layer;
use tokio::sync::broadcast;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, reload::Handle, Registry};

pub struct SubscriberHandles {
    pub filter_reload: Handle<EnvFilter, Registry>,
    pub fanout: FanoutLayerHandle,
    pub broadcast_rx: broadcast::Receiver<LoggedEvent>,
}

pub fn build() -> (impl tracing::Subscriber + Send + Sync, SubscriberHandles) {
    let (filter, filter_reload) = filter_layer::build();
    let (fanout, broadcast_rx) = FanoutLayer::create();

    let subscriber = Registry::default().with(filter).with(fanout.clone());

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

    #[test]
    fn build_returns_subscriber_and_handles() {
        let (_sub, _handles) = build();
    }
}
