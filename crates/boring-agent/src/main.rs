mod git;
mod workspace;

use anyhow::Context;
use boring_broker::{Broker, NatsBroker};
use boring_proto::event::Event;
use boring_store::{S3Store, Store};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, error};

struct AgentEnv {
    run_id: String,
    hat_id: String,
    event_topic: String,
    event_payload: String,
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
    git_repo: Option<String>,
    git_base_branch: String,
    git_branch_strategy: String,
    git_token: Option<String>,
}

impl AgentEnv {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            run_id: std::env::var("BORING_RUN_ID").context("BORING_RUN_ID not set")?,
            hat_id: std::env::var("BORING_HAT_ID").context("BORING_HAT_ID not set")?,
            event_topic: std::env::var("BORING_EVENT_TOPIC").unwrap_or_default(),
            event_payload: std::env::var("BORING_EVENT_PAYLOAD").unwrap_or_default(),
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
            git_repo: std::env::var("BORING_GIT_REPO").ok(),
            git_base_branch: std::env::var("BORING_GIT_BASE_BRANCH")
                .unwrap_or_else(|_| "main".to_string()),
            git_branch_strategy: std::env::var("BORING_GIT_BRANCH_STRATEGY")
                .unwrap_or_else(|_| "shared".to_string()),
            git_token: std::env::var("BORING_GIT_TOKEN").ok(),
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

    // ── Phase 1: Connect to S3 ──────────────────────────────────
    let store = S3Store::local(
        &env.store_endpoint,
        &env.store_bucket,
        &env.store_prefix,
        &env.store_access_key,
        &env.store_secret_key,
    )
    .await
    .context("failed to connect to S3 store")?;

    // ── Phase 2: Set up working directory ───────────────────────
    let (work_dir, work_branch) = if let Some(ref repo) = env.git_repo {
        let branch = match env.git_branch_strategy.as_str() {
            "per_hat" => format!("bore/{}/{}", env.run_id, env.hat_id),
            _ => format!("bore/{}/main", env.run_id),
        };

        let target = std::env::temp_dir().join(format!("boring-{}-{}", env.run_id, env.hat_id));
        git::clone_and_checkout(repo, &env.git_base_branch, &branch, &target, env.git_token.as_deref())
            .await
            .context("git clone failed")?;

        (target, Some(branch))
    } else {
        // No git repo — use a temp dir as the working directory
        let target = std::env::temp_dir().join(format!("boring-{}-{}", env.run_id, env.hat_id));
        tokio::fs::create_dir_all(&target).await?;
        (target, None)
    };

    // ── Phase 3: Materialize S3 state into .boring/ ─────────────
    // This is the key step: everything the agent needs is now on disk
    // where grep, cat, and the AI CLI can find it.
    workspace::materialize(
        &store,
        &env.run_id,
        &env.hat_id,
        &work_dir,
        &env.prompt,
        &env.event_topic,
        &env.event_payload,
    )
    .await
    .context("failed to materialize workspace")?;

    // ── Phase 4: Run the agent command ──────────────────────────
    let command = env.command.unwrap_or_else(|| {
        "echo \"$BORING_PROMPT\"".to_string()
    });

    info!(command = %command, "executing agent command");

    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .env("BORING_PROMPT", &env.prompt)
        .env("BORING_EVENT_TOPIC", &env.event_topic)
        .env("BORING_EVENT_PAYLOAD", &env.event_payload)
        .env("BORING_WORKSPACE", work_dir.join(".boring").to_string_lossy().as_ref())
        .current_dir(&work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .await
        .context("failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() {
        error!(exit_code = output.status.code(), "command failed");
        std::process::exit(1);
    }

    info!("command completed, parsing output");

    // ── Phase 5: Sync .boring/ back to S3 ───────────────────────
    // The agent may have modified scratchpads, memories, or tasks
    workspace::sync_back(&store, &env.run_id, &env.hat_id, &work_dir)
        .await
        .context("failed to sync workspace back to S3")?;

    // ── Phase 6: Read emit file and publish to NATS ──────────────
    // The `emit` CLI tool writes JSONL to this file. No more stdout parsing.
    let emit_file = std::path::PathBuf::from(
        std::env::var("BORING_EMIT_FILE").unwrap_or_else(|_| "/tmp/boring-emits.jsonl".to_string())
    );
    let broker = NatsBroker::new(&env.broker_url, &env.broker_stream)
        .await
        .context("failed to connect to NATS")?;

    let mut seq = 0u64;

    if emit_file.exists() {
        let content = tokio::fs::read_to_string(&emit_file).await?;
        for line in content.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                match v.get("type").and_then(|t| t.as_str()) {
                    Some("event") => {
                        let topic = v.get("topic").and_then(|t| t.as_str()).unwrap_or("");
                        let payload = v.get("payload").and_then(|p| p.as_str()).unwrap_or("");
                        if !topic.is_empty() {
                            let event = Event::new(topic, payload, Some(&env.hat_id), &env.run_id, seq);
                            broker.publish(&env.run_id, &event).await?;
                            info!(topic = %topic, payload = %payload, "published event");
                            seq += 1;
                        }
                    }
                    Some("complete") => {
                        let promise = v.get("promise").and_then(|p| p.as_str())
                            .unwrap_or(&env.completion_promise);
                        let event = Event::system_completion(&env.run_id, promise, seq);
                        broker.publish(&env.run_id, &event).await?;
                        info!("published completion event");
                        seq += 1;
                    }
                    _ => {
                        info!(line = %line.trim(), "emit recorded");
                    }
                }
            }
        }
    } else {
        info!("no emit file found — agent did not call `emit`");
    }

    // ── Phase 7: Push git changes (with conflict retry) ──────────
    if let Some(ref branch) = work_branch {
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&work_dir)
            .status()
            .await;
        let _ = Command::new("git")
            .args(["commit", "-m", &format!("boring: {} iteration", env.hat_id)])
            .current_dir(&work_dir)
            .status()
            .await;

        let max_retries = 3;
        for attempt in 0..max_retries {
            match git::push(&work_dir, branch).await {
                Ok(true) => {
                    info!("pushed changes to {}", branch);
                    break;
                }
                Ok(false) => {
                    info!("no changes to push");
                    break;
                }
                Err(e) => {
                    if attempt + 1 >= max_retries {
                        error!("git push failed after {} attempts: {}", max_retries, e);
                        break;
                    }
                    info!(attempt = attempt + 1, "push failed, asking Claude to fix conflicts");

                    // Re-invoke Claude to fix the rebase conflicts
                    let fix_output = Command::new("bash")
                        .arg("-c")
                        .arg("claude --print --dangerously-skip-permissions -p 'There are git rebase conflicts. Run git status to see them, fix all conflicted files, then run: git add -A && git rebase --continue'")
                        .current_dir(&work_dir)
                        .output()
                        .await;

                    match fix_output {
                        Ok(o) if o.status.success() => {
                            info!("Claude fixed conflicts, retrying push");
                        }
                        _ => {
                            error!("Claude failed to fix conflicts");
                            break;
                        }
                    }
                }
            }
        }
    }

    info!(events_published = seq, "boring-agent finished");
    Ok(())
}
