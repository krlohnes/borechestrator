use std::process::ExitCode;
use boring_proto::event::Event;
use boring_broker::NatsBroker;
use boring_broker::Broker;

pub async fn run(run_id: &str, topic: &str, payload: &str) -> ExitCode {
    let broker = match NatsBroker::new("nats://127.0.0.1:4222", "BORING").await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to connect to NATS: {}", e);
            return ExitCode::from(1);
        }
    };

    let event = Event::new(topic, payload, None, run_id, 0);

    match broker.publish(run_id, &event).await {
        Ok(()) => {
            println!("Event published: {} -> {}", topic, payload);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Failed to publish event: {}", e);
            ExitCode::from(1)
        }
    }
}
