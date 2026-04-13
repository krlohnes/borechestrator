use boring_broker::{Broker, NatsBroker};
use boring_proto::event::Event;
use tokio::time::{timeout, Duration};

async fn setup_broker() -> NatsBroker {
    NatsBroker::new("nats://127.0.0.1:4222", "BORING_TEST")
        .await
        .expect("NATS must be running for integration tests")
}

#[tokio::test]
#[ignore] // requires nats-server
async fn test_publish_and_subscribe() {
    let broker = setup_broker().await;
    let run_id = &format!("test-{}", uuid::Uuid::new_v4());

    let mut sub = broker.subscribe_all(run_id).await.unwrap();
    let event = Event::new("work.start", "hello", None, run_id, 1);
    broker.publish(run_id, &event).await.unwrap();

    let received = timeout(Duration::from_secs(5), sub.next())
        .await
        .expect("timed out waiting for event")
        .expect("subscription ended unexpectedly");

    assert_eq!(received.topic, "work.start");
    assert_eq!(received.payload, "hello");
    assert_eq!(received.run_id, run_id.to_string());
}

#[tokio::test]
#[ignore]
async fn test_wildcard_subscription_filters() {
    let broker = setup_broker().await;
    let run_id = &format!("test-{}", uuid::Uuid::new_v4());

    let mut sub = broker.subscribe(run_id, "work.*").await.unwrap();

    let match_event = Event::new("work.start", "yes", None, run_id, 1);
    broker.publish(run_id, &match_event).await.unwrap();

    let no_match = Event::new("other.thing", "no", None, run_id, 2);
    broker.publish(run_id, &no_match).await.unwrap();

    let received = timeout(Duration::from_secs(5), sub.next())
        .await
        .expect("timed out")
        .expect("ended");
    assert_eq!(received.topic, "work.start");

    let second = timeout(Duration::from_millis(500), sub.next()).await;
    assert!(
        second.is_err(),
        "should not have received non-matching event"
    );
}

#[tokio::test]
#[ignore]
async fn test_multiple_events_ordered() {
    let broker = setup_broker().await;
    let run_id = &format!("test-{}", uuid::Uuid::new_v4());

    let mut sub = broker.subscribe_all(run_id).await.unwrap();

    for i in 0..3u64 {
        let event = Event::new("work.step", &format!("step-{}", i), None, run_id, i);
        broker.publish(run_id, &event).await.unwrap();
    }

    for i in 0..3u64 {
        let received = timeout(Duration::from_secs(5), sub.next())
            .await
            .expect("timed out")
            .expect("ended");
        assert_eq!(received.payload, format!("step-{}", i));
        assert_eq!(received.sequence, i);
    }
}
