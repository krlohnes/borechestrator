use anyhow::Context;
use boring_broker::{Broker, NatsBroker};
use boring_proto::event::Event;
use boring_store::{S3Store, Store};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, error};

/// Environment variable names used by boring-agent.
struct AgentEnv {
    run_id: String,
    hat_id: String,
    event_topic: String,
    event_payload: String,
    instructions: String,
    completion_promise: String,
    prompt: String,
    broker_url: String,
    broker_stream: String,
    store_endpoint: String,
    store_bucket: String,
    store_prefix: String,
    store_access_key: String,
    store_secret_key: String,
    command: Option<String>,
}

impl AgentEnv {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            run_id: std::env::var("BORING_RUN_ID").context("BORING_RUN_ID not set")?,
            hat_id: std::env::var("BORING_HAT_ID").context("BORING_HAT_ID not set")?,
            event_topic: std::env::var("BORING_EVENT_TOPIC").unwrap_or_default(),
            event_payload: std::env::var("BORING_EVENT_PAYLOAD").unwrap_or_default(),
            instructions: std::env::var("BORING_INSTRUCTIONS").unwrap_or_default(),
            completion_promise: std::env::var("BORING_COMPLETION_PROMISE")
                .unwrap_or_else(|_| "LOOP_COMPLETE".to_string()),
            prompt: std::env::var("BORING_PROMPT").unwrap_or_default(),
            broker_url: std::env::var("BORING_BROKER_URL")
                .unwrap_or_else(|_| "nats://nats:4222".to_string()),
            broker_stream: std::env::var("BORING_BROKER_STREAM")
                .unwrap_or_else(|_| "BORING".to_string()),
            store_endpoint: std::env::var("BORING_STORE_ENDPOINT")
                .unwrap_or_else(|_| "http://rustfs:9000".to_string()),
            store_bucket: std::env::var("BORING_STORE_BUCKET")
                .unwrap_or_else(|_| "borechestrator".to_string()),
            store_prefix: std::env::var("BORING_STORE_PREFIX").unwrap_or_default(),
            store_access_key: std::env::var("BORING_STORE_ACCESS_KEY")
                .unwrap_or_else(|_| "rustfsadmin".to_string()),
            store_secret_key: std::env::var("BORING_STORE_SECRET_KEY")
                .unwrap_or_else(|_| "rustfsadmin".to_string()),
            command: std::env::var("BORING_COMMAND").ok(),
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("boring_agent=info".parse().unwrap()),
        )
        .json()
        .init();

    let env = AgentEnv::from_env()?;

    info!(
        run_id = %env.run_id,
        hat_id = %env.hat_id,
        event_topic = %env.event_topic,
        "boring-agent starting"
    );

    // Connect to S3 store
    let store = S3Store::local(
        &env.store_endpoint,
        &env.store_bucket,
        &env.store_prefix,
        &env.store_access_key,
        &env.store_secret_key,
    )
    .await
    .context("failed to connect to S3 store")?;

    // Download scratchpad
    let scratchpad_key = format!("{}/scratchpad/{}.md", env.run_id, env.hat_id);
    let scratchpad = match store.get(&scratchpad_key).await? {
        Some(bytes) => {
            let content = String::from_utf8_lossy(&bytes).to_string();
            info!(key = %scratchpad_key, "downloaded scratchpad ({} bytes)", content.len());
            Some(content)
        }
        None => {
            info!(key = %scratchpad_key, "no scratchpad found");
            None
        }
    };

    // Determine what command to run
    let command = env.command.unwrap_or_else(|| {
        // Default: pipe the prompt to the AI CLI via echo
        "echo \"$BORING_PROMPT\"".to_string()
    });

    info!(command = %command, "executing agent command");

    // Run the command
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .env("BORING_PROMPT", &env.prompt)
        .env("BORING_SCRATCHPAD", scratchpad.as_deref().unwrap_or(""))
        .env("BORING_EVENT_TOPIC", &env.event_topic)
        .env("BORING_EVENT_PAYLOAD", &env.event_payload)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .await
        .context("failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() {
        error!(
            exit_code = output.status.code(),
            "command failed"
        );
        std::process::exit(1);
    }

    info!("command completed, parsing output");

    // Connect to NATS broker
    let broker = NatsBroker::new(&env.broker_url, &env.broker_stream)
        .await
        .context("failed to connect to NATS")?;

    // Parse stdout for BORING_EMIT lines and completion promise
    let mut seq = 0u64;
    let mut scratchpad_updates = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("BORING_EMIT ") {
            let mut parts = rest.splitn(2, ' ');
            if let Some(topic) = parts.next() {
                let payload = parts.next().unwrap_or("");
                if !topic.is_empty() {
                    let event = Event::new(topic, payload, Some(&env.hat_id), &env.run_id, seq);
                    broker.publish(&env.run_id, &event).await?;
                    info!(topic = %topic, payload = %payload, "published event");
                    seq += 1;
                }
            }
        } else if trimmed.contains(&env.completion_promise) {
            let event = Event::system_completion(&env.run_id, &env.completion_promise, seq);
            broker.publish(&env.run_id, &event).await?;
            info!("published completion event");
            seq += 1;
        } else if let Some(rest) = trimmed.strip_prefix("BORING_SCRATCHPAD ") {
            scratchpad_updates.push(rest.to_string());
        }
    }

    // Upload updated scratchpad if there were updates
    if !scratchpad_updates.is_empty() {
        let new_content = scratchpad_updates.join("\n");
        let full_content = match scratchpad {
            Some(existing) => format!("{}\n{}", existing, new_content),
            None => new_content,
        };
        store
            .put(&scratchpad_key, full_content.into_bytes())
            .await?;
        info!(key = %scratchpad_key, "uploaded scratchpad");
    }

    info!(events_published = seq, "boring-agent finished");
    Ok(())
}
