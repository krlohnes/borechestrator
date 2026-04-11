use anyhow::Context;
use async_trait::async_trait;
use boring_proto::event::Event;
use futures_util::StreamExt;

use crate::traits::{Broker, Subscription};

/// Construct a NATS subject from a run_id and topic.
pub fn nats_subject(run_id: &str, topic: &str) -> String {
    format!("boring.{}.{}", run_id, topic)
}

/// Construct a NATS subscription pattern from a run_id and topic pattern.
pub fn nats_subscribe_pattern(run_id: &str, pattern: &str) -> String {
    format!("boring.{}.{}", run_id, pattern)
}

/// Construct a NATS subscription for all events in a run.
pub fn nats_subscribe_all(run_id: &str) -> String {
    format!("boring.{}.>", run_id)
}

/// NATS JetStream broker implementation.
pub struct NatsBroker {
    client: async_nats::Client,
    jetstream: async_nats::jetstream::Context,
    stream_name: String,
}

impl NatsBroker {
    /// Connect to NATS and ensure the JetStream stream exists.
    pub async fn new(url: &str, stream_name: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(url)
            .await
            .context("failed to connect to NATS")?;

        let jetstream = async_nats::jetstream::new(client.clone());

        // Create or get the stream
        jetstream
            .get_or_create_stream(async_nats::jetstream::stream::Config {
                name: stream_name.to_string(),
                subjects: vec!["boring.>".to_string()],
                ..Default::default()
            })
            .await
            .context("failed to create/get JetStream stream")?;

        Ok(Self {
            client,
            jetstream,
            stream_name: stream_name.to_string(),
        })
    }
}

#[async_trait]
impl Broker for NatsBroker {
    async fn publish(&self, run_id: &str, event: &Event) -> anyhow::Result<()> {
        let subject = nats_subject(run_id, &event.topic);
        let payload = serde_json::to_vec(event).context("failed to serialize event")?;
        self.jetstream
            .publish(subject, payload.into())
            .await
            .context("failed to publish to JetStream")?
            .await
            .context("failed to confirm publish")?;
        Ok(())
    }

    async fn subscribe(&self, run_id: &str, pattern: &str) -> anyhow::Result<Subscription> {
        let subject = nats_subscribe_pattern(run_id, pattern);
        let (tx, rx) = tokio::sync::mpsc::channel(256);

        let mut subscriber = self
            .client
            .subscribe(subject)
            .await
            .context("failed to subscribe")?;

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                if let Ok(event) = serde_json::from_slice::<Event>(&msg.payload) {
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Subscription::new(rx))
    }

    async fn subscribe_all(&self, run_id: &str) -> anyhow::Result<Subscription> {
        let subject = nats_subscribe_all(run_id);
        let (tx, rx) = tokio::sync::mpsc::channel(256);

        let mut subscriber = self
            .client
            .subscribe(subject)
            .await
            .context("failed to subscribe")?;

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                if let Ok(event) = serde_json::from_slice::<Event>(&msg.payload) {
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Subscription::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nats_subject_from_topic() {
        assert_eq!(nats_subject("run-abc", "work.start"), "boring.run-abc.work.start");
    }

    #[test]
    fn test_nats_subject_single_segment() {
        assert_eq!(nats_subject("run-abc", "start"), "boring.run-abc.start");
    }

    #[test]
    fn test_nats_subscribe_pattern_wildcard() {
        assert_eq!(
            nats_subscribe_pattern("run-abc", "work.*"),
            "boring.run-abc.work.*"
        );
    }

    #[test]
    fn test_nats_subscribe_pattern_multi_wildcard() {
        assert_eq!(nats_subscribe_pattern("run-abc", ">"), "boring.run-abc.>");
    }

    #[test]
    fn test_nats_subscribe_all() {
        assert_eq!(nats_subscribe_all("run-abc"), "boring.run-abc.>");
    }
}
