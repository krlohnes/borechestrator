use async_trait::async_trait;
use boring_proto::event::Event;

/// Abstraction over a message broker for inter-hat event routing.
#[async_trait]
pub trait Broker: Send + Sync {
    /// Publish an event to the broker under the given run.
    async fn publish(&self, run_id: &str, event: &Event) -> anyhow::Result<()>;

    /// Subscribe to events matching a topic pattern for the given run.
    async fn subscribe(&self, run_id: &str, pattern: &str) -> anyhow::Result<Subscription>;

    /// Subscribe to all events for the given run.
    async fn subscribe_all(&self, run_id: &str) -> anyhow::Result<Subscription>;
}

/// An async stream of events from a subscription.
pub struct Subscription {
    rx: tokio::sync::mpsc::Receiver<Event>,
}

impl Subscription {
    pub fn new(rx: tokio::sync::mpsc::Receiver<Event>) -> Self {
        Self { rx }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}
