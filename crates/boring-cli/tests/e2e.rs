use boring_broker::NatsBroker;
use boring_controller::reconciler::{Reconciler, RunResult};
use boring_proto::config::BoringConfig;
use boring_runtime::LocalRuntime;
use boring_secrets::EnvSecretProvider;
use boring_store::LocalStore;
use tempfile::TempDir;

/// Full end-to-end test: two echo-based hats, real NATS, real local runtime.
///
/// Pipeline:
///   work.start → planner (echoes BORING_EMIT subtask.ready) → builder (echoes LOOP_COMPLETE)
#[tokio::test]
#[ignore] // requires nats-server running on localhost:4222
async fn test_e2e_two_hat_pipeline() {
    let config = BoringConfig::from_yaml(
        r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 10
  max_runtime_seconds: 30

hats:
  planner:
    name: Planner
    description: "Emits a subtask"
    triggers: ["work.start"]
    publishes: ["subtask.ready"]
    command: "echo 'Planning...' && echo 'BORING_EMIT subtask.ready build the thing'"
    instructions: "Plan the work."
  builder:
    name: Builder
    description: "Does the work and completes"
    triggers: ["subtask.ready"]
    publishes: ["work.done"]
    command: "echo 'Building...' && echo 'LOOP_COMPLETE'"
    instructions: "Build it."
"#,
    )
    .unwrap();

    assert!(config.validate().is_ok());

    let broker = NatsBroker::new("nats://127.0.0.1:4222", "BORING_TEST")
        .await
        .expect("NATS must be running");

    let store_dir = TempDir::new().unwrap();
    let store = LocalStore::new(store_dir.path());
    let runtime = LocalRuntime::new();

    let mut reconciler = Reconciler::new(
        config,
        Box::new(broker),
        Box::new(store),
        Box::new(runtime),
        Box::new(EnvSecretProvider::new()),
    );

    let result = reconciler.run().await.unwrap();

    match result {
        RunResult::Completed => {} // success!
        other => panic!("Expected Completed, got {:?}", other),
    }
}

/// Single hat that immediately completes.
#[tokio::test]
#[ignore]
async fn test_e2e_single_hat_immediate_completion() {
    let config = BoringConfig::from_yaml(
        r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 5
  max_runtime_seconds: 10

hats:
  worker:
    name: Worker
    description: "Completes immediately"
    triggers: ["work.start"]
    publishes: []
    command: "echo 'LOOP_COMPLETE'"
    instructions: "Just complete."
"#,
    )
    .unwrap();

    let broker = NatsBroker::new("nats://127.0.0.1:4222", "BORING_TEST")
        .await
        .expect("NATS must be running");

    let store_dir = TempDir::new().unwrap();
    let store = LocalStore::new(store_dir.path());
    let runtime = LocalRuntime::new();

    let mut reconciler = Reconciler::new(
        config,
        Box::new(broker),
        Box::new(store),
        Box::new(runtime),
        Box::new(EnvSecretProvider::new()),
    );

    let result = reconciler.run().await.unwrap();
    match result {
        RunResult::Completed => {}
        other => panic!("Expected Completed, got {:?}", other),
    }
}

/// Three-hat pipeline: planner → builder → verifier → LOOP_COMPLETE
#[tokio::test]
#[ignore]
async fn test_e2e_three_hat_pipeline() {
    let config = BoringConfig::from_yaml(
        r#"
event_loop:
  starting_event: pipeline.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 10
  max_runtime_seconds: 30

hats:
  planner:
    name: Planner
    description: "Plans"
    triggers: ["pipeline.start"]
    publishes: ["build.ready"]
    command: "echo 'BORING_EMIT build.ready implement feature X'"
    instructions: "Plan."
  builder:
    name: Builder
    description: "Builds"
    triggers: ["build.ready"]
    publishes: ["verify.ready"]
    command: "echo 'BORING_EMIT verify.ready code written'"
    instructions: "Build."
  verifier:
    name: Verifier
    description: "Verifies and completes"
    triggers: ["verify.ready"]
    publishes: []
    command: "echo 'All checks pass' && echo 'LOOP_COMPLETE'"
    instructions: "Verify."
"#,
    )
    .unwrap();

    let broker = NatsBroker::new("nats://127.0.0.1:4222", "BORING_TEST")
        .await
        .expect("NATS must be running");

    let store_dir = TempDir::new().unwrap();
    let store = LocalStore::new(store_dir.path());
    let runtime = LocalRuntime::new();

    let mut reconciler = Reconciler::new(
        config,
        Box::new(broker),
        Box::new(store),
        Box::new(runtime),
        Box::new(EnvSecretProvider::new()),
    );

    let result = reconciler.run().await.unwrap();
    match result {
        RunResult::Completed => {}
        other => panic!("Expected Completed, got {:?}", other),
    }
}
