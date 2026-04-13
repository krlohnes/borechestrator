use boring_broker::Broker;
use boring_proto::config::BoringConfig;
use boring_proto::event::Event;
use boring_runtime::{JobStatus, Runtime};
use boring_secrets::SecretProvider;
use boring_store::Store;
use std::collections::{HashMap, HashSet};

use crate::backpressure;
use crate::event_router::EventRouter;
use crate::job_builder::JobBuilder;
use crate::output_parser;

/// Result of a reconciliation run.
#[derive(Debug)]
pub enum RunResult {
    Completed,
    MaxIterationsReached,
    TimedOut,
    Failed { reason: String },
}

/// Callback that extracts output events from a completed job.
/// In production, this is a no-op (agents publish directly to NATS).
/// In tests, this extracts events from the FakeRuntime.
pub type EventExtractor = Box<dyn Fn(&str) -> Vec<Event> + Send + Sync>;

/// The main orchestration loop.
pub struct Reconciler {
    config: BoringConfig,
    broker: Box<dyn Broker>,
    store: Box<dyn Store>,
    runtime: Box<dyn Runtime>,
    secrets: Box<dyn SecretProvider>,
    event_extractor: Option<EventExtractor>,
}

impl Reconciler {
    pub fn new(
        config: BoringConfig,
        broker: Box<dyn Broker>,
        store: Box<dyn Store>,
        runtime: Box<dyn Runtime>,
        secrets: Box<dyn SecretProvider>,
    ) -> Self {
        Self {
            config,
            broker,
            store,
            runtime,
            secrets,
            event_extractor: None,
        }
    }

    /// Set an event extractor for testing. Called after each successful job
    /// to get events the "agent" would have published.
    pub fn with_event_extractor(mut self, extractor: EventExtractor) -> Self {
        self.event_extractor = Some(extractor);
        self
    }

    /// Run a new orchestration.
    pub async fn run(&mut self) -> anyhow::Result<RunResult> {
        self.run_inner(None).await
    }

    /// Resume from the last checkpoint.
    pub async fn resume(&mut self) -> anyhow::Result<RunResult> {
        let checkpoint = crate::checkpoint::Checkpoint::find_latest(&*self.store).await?;
        match checkpoint {
            Some(cp) => {
                tracing::info!(run_id = %cp.run_id, iterations = cp.iterations, "resuming from checkpoint");
                self.run_inner(Some(cp)).await
            }
            None => {
                tracing::warn!("no checkpoint found, starting fresh");
                self.run_inner(None).await
            }
        }
    }

    async fn run_inner(
        &mut self,
        checkpoint: Option<crate::checkpoint::Checkpoint>,
    ) -> anyhow::Result<RunResult> {
        let (
            run_id,
            mut iterations,
            mut activations,
            mut consecutive_failures,
            mut global_sequence,
            mut seen_events,
        ) = if let Some(cp) = checkpoint {
            (
                cp.run_id,
                cp.iterations,
                cp.activations,
                cp.consecutive_failures,
                cp.global_sequence,
                cp.seen_events,
            )
        } else {
            (
                format!(
                    "run-{}",
                    uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
                ),
                0u32,
                HashMap::new(),
                0u32,
                1u64,
                Vec::new(),
            )
        };

        let router = EventRouter::new(self.config.hats.clone());
        let builder = JobBuilder::new(&self.config);
        let max_iterations = self.config.event_loop.max_iterations.unwrap_or(100);
        let completion_promise = self.config.event_loop.completion_promise.clone();
        let checkpoint_interval = self.config.event_loop.checkpoint_interval.unwrap_or(5);

        let mut active_jobs: HashSet<String> = HashSet::new();

        // Subscribe BEFORE publishing so we don't miss the starting event
        let mut subscription = self.broker.subscribe_all(&run_id).await?;

        // Only publish starting event for fresh runs (not resumes)
        if seen_events.is_empty() {
            let starting_event =
                Event::new(&self.config.event_loop.starting_event, "", None, &run_id, 0);
            self.broker.publish(&run_id, &starting_event).await?;
        }

        loop {
            if iterations >= max_iterations {
                return Ok(RunResult::MaxIterationsReached);
            }

            let event = match tokio::time::timeout(
                std::time::Duration::from_secs(
                    self.config.event_loop.max_runtime_seconds.unwrap_or(14400),
                ),
                subscription.next(),
            )
            .await
            {
                Ok(Some(event)) => event,
                Ok(None) => {
                    return Ok(RunResult::Failed {
                        reason: "subscription ended".to_string(),
                    });
                }
                Err(_) => return Ok(RunResult::TimedOut),
            };

            // Track seen events for checkpointing
            if !event.is_system() {
                seen_events.push(event.topic.clone());
            }

            if event.is_completion(&completion_promise) {
                return Ok(RunResult::Completed);
            }

            let hat_ids = router.route_with_state(&event, &activations);

            // Phase 1: Create all jobs concurrently (hats run in parallel)
            let mut pending_handles: Vec<(String, boring_runtime::JobHandle)> = Vec::new();

            for hat_id in hat_ids {
                if active_jobs.contains(&hat_id) {
                    continue;
                }

                let hat = match self.config.hats.get(&hat_id) {
                    Some(h) => h,
                    None => continue,
                };

                // Run global + per-hat backpressure gates
                let mut gates = self
                    .config
                    .backpressure
                    .as_ref()
                    .map(|bp| bp.gates.clone())
                    .unwrap_or_default();
                gates.extend(hat.gates.clone());

                if !gates.is_empty() {
                    if let Err(failure) = backpressure::run_gates(&gates).await {
                        tracing::warn!(hat = %hat_id, gate = %failure.gate_name, "gate failed, skipping hat");
                        consecutive_failures += 1;
                        iterations += 1;
                        if consecutive_failures >= 3 {
                            return Ok(RunResult::Failed {
                                reason: format!("gate failure: {}", failure),
                            });
                        }
                        continue;
                    }
                }

                let scratchpad_key = format!("{}/scratchpad/{}.md", run_id, hat_id);
                let scratchpad = self
                    .store
                    .get(&scratchpad_key)
                    .await
                    .ok()
                    .flatten()
                    .map(|bytes| String::from_utf8_lossy(&bytes).to_string());

                let spec = builder
                    .build(&hat_id, hat, &event, scratchpad.as_deref(), &*self.secrets)
                    .await?;

                let handle = self.runtime.create_job(spec).await?;
                active_jobs.insert(hat_id.clone());
                iterations += 1;
                *activations.entry(hat_id.clone()).or_insert(0) += 1;

                pending_handles.push((hat_id, handle));
            }

            // Phase 2: Wait for all jobs to complete
            // Jobs are already running concurrently; we poll each one.
            for (hat_id, handle) in pending_handles {
                let status = self.runtime.wait_job(&handle).await?;
                active_jobs.remove(&hat_id);

                match status {
                    JobStatus::Succeeded { ref stdout } => {
                        consecutive_failures = 0;

                        let parsed = output_parser::parse_output(
                            stdout,
                            &hat_id,
                            &run_id,
                            &completion_promise,
                            global_sequence,
                        );
                        global_sequence += parsed.events.len() as u64;

                        // Publish events to broker
                        if !parsed.events.is_empty() {
                            for evt in &parsed.events {
                                self.broker.publish(&run_id, evt).await?;
                            }
                        } else if let Some(ref extractor) = self.event_extractor {
                            // Test fakes
                            let fake_events = extractor(&handle.id);
                            for evt in fake_events {
                                self.broker.publish(&run_id, &evt).await?;
                            }
                        } else {
                            // Check if stdout contains completion promise
                            let stripped_lines: Vec<&str> = stdout
                                .lines()
                                .map(|l| {
                                    l.trim().trim_matches(|c: char| {
                                        c == '*'
                                            || c == '`'
                                            || c == '_'
                                            || c == '#'
                                            || c.is_whitespace()
                                    })
                                })
                                .collect();
                            if stripped_lines.iter().any(|l| *l == completion_promise) {
                                tracing::info!(hat = %hat_id, "completion promise found in stdout");
                                let evt = Event::system_completion(
                                    &run_id,
                                    &completion_promise,
                                    global_sequence,
                                );
                                global_sequence += 1;
                                self.broker.publish(&run_id, &evt).await?;
                            } else {
                                // Auto-emit: emit the hat's default publish to keep the pipeline moving.
                                let hat = self.config.hats.get(&hat_id);
                                let default_topic = hat.and_then(|h| {
                                    h.default_publishes.as_ref().or(h.publishes.first())
                                });
                                if let Some(topic) = default_topic {
                                    tracing::info!(hat = %hat_id, topic = %topic, "auto-emitting default event");
                                    let evt = Event::new(
                                        topic,
                                        "auto-emitted",
                                        Some(&hat_id),
                                        &run_id,
                                        global_sequence,
                                    );
                                    global_sequence += 1;
                                    self.broker.publish(&run_id, &evt).await?;
                                }
                            }
                        }

                        // Persist memories
                        if !parsed.memories.is_empty() {
                            let mem_store = crate::memories::MemoryStore::new(&run_id);
                            for memory in parsed.memories {
                                mem_store.append(&*self.store, memory).await.ok();
                            }
                        }

                        // Persist task actions
                        if !parsed.task_actions.is_empty() {
                            let task_store = crate::tasks::TaskStore::new(&run_id);
                            for action in parsed.task_actions {
                                match action {
                                    crate::tasks::TaskAction::Add(task) => {
                                        task_store.add(&*self.store, task).await.ok();
                                    }
                                    crate::tasks::TaskAction::Done(id) => {
                                        task_store
                                            .update_status(
                                                &*self.store,
                                                &id,
                                                crate::tasks::TaskStatus::Done,
                                            )
                                            .await
                                            .ok();
                                    }
                                    crate::tasks::TaskAction::InProgress(id) => {
                                        task_store
                                            .update_status(
                                                &*self.store,
                                                &id,
                                                crate::tasks::TaskStatus::InProgress,
                                            )
                                            .await
                                            .ok();
                                    }
                                }
                            }
                        }

                        // Persist scratchpad updates
                        if !parsed.scratchpad_lines.is_empty() {
                            let sp_key = format!("{}/scratchpad/{}.md", run_id, hat_id);
                            let existing = self
                                .store
                                .get(&sp_key)
                                .await
                                .ok()
                                .flatten()
                                .map(|b| String::from_utf8_lossy(&b).to_string())
                                .unwrap_or_default();
                            let new_content =
                                format!("{}\n{}", existing, parsed.scratchpad_lines.join("\n"));
                            self.store.put(&sp_key, new_content.into_bytes()).await.ok();
                        }
                    }
                    JobStatus::Failed { reason, .. } => {
                        consecutive_failures += 1;
                        if consecutive_failures >= 3 {
                            return Ok(RunResult::Failed { reason });
                        }
                    }
                }
            }

            // Checkpoint every N iterations
            if checkpoint_interval > 0 && iterations % checkpoint_interval == 0 {
                let cp = crate::checkpoint::Checkpoint {
                    run_id: run_id.clone(),
                    iterations,
                    activations: activations.clone(),
                    consecutive_failures,
                    global_sequence,
                    seen_events: seen_events.clone(),
                    config_hash: String::new(),
                };
                cp.save(&*self.store).await.ok();
                tracing::debug!(iterations, "checkpoint saved");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use boring_runtime::JobSpec;
    use boring_secrets::NoopSecretProvider;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn two_hat_config() -> BoringConfig {
        BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 10

hats:
  planner:
    name: Planner
    description: "Plans work"
    triggers: ["work.start"]
    publishes: ["subtask.ready"]
    instructions: "Plan the work."
  builder:
    name: Builder
    description: "Builds things"
    triggers: ["subtask.ready"]
    publishes: ["work.done"]
    instructions: "Build it."
"#,
        )
        .unwrap()
    }

    /// Build a reconciler with a FakeRuntime wired to publish events through the broker.
    fn build_reconciler(
        config: BoringConfig,
        broker: FakeBroker,
        store: FakeStore,
        runtime: FakeRuntime,
    ) -> (Reconciler, Arc<Mutex<Vec<JobSpec>>>) {
        let jobs = runtime.created_jobs();
        let pending = runtime.pending_events.clone();

        let extractor: EventExtractor = Box::new(move |handle_id: &str| {
            // Use try_lock since we're inside a sync closure
            // The pending events were inserted during create_job
            let mut lock = pending.try_lock().expect("lock should be available");
            lock.remove(handle_id).unwrap_or_default()
        });

        let reconciler = Reconciler::new(
            config,
            Box::new(broker),
            Box::new(store),
            Box::new(runtime),
            Box::new(NoopSecretProvider),
        )
        .with_event_extractor(extractor);

        (reconciler, jobs)
    }

    #[tokio::test]
    async fn test_publishes_starting_event() {
        let config = two_hat_config();
        let broker = FakeBroker::new();
        let published = broker.published_events();

        let runtime = FakeRuntime::new();
        runtime.set_response(
            "planner",
            JobResponse::succeed_with_events(vec![("_system.completion", "LOOP_COMPLETE")]),
        );

        let (mut reconciler, _) = build_reconciler(config, broker, FakeStore::new(), runtime);
        reconciler.run().await.unwrap();

        let events = published.lock().await;
        assert_eq!(events[0].topic, "work.start");
    }

    #[tokio::test]
    async fn test_routes_starting_event_to_correct_hat() {
        let config = two_hat_config();
        let runtime = FakeRuntime::new();
        runtime.set_response(
            "planner",
            JobResponse::succeed_with_events(vec![("_system.completion", "LOOP_COMPLETE")]),
        );

        let (mut reconciler, jobs) =
            build_reconciler(config, FakeBroker::new(), FakeStore::new(), runtime);
        reconciler.run().await.unwrap();

        let jobs = jobs.lock().await;
        assert!(jobs.iter().any(|j| j.hat_id == "planner"));
    }

    #[tokio::test]
    async fn test_completion_stops_loop() {
        let config = two_hat_config();
        let runtime = FakeRuntime::new();
        runtime.set_response(
            "planner",
            JobResponse::succeed_with_events(vec![("subtask.ready", "do the thing")]),
        );
        runtime.set_response(
            "builder",
            JobResponse::succeed_with_events(vec![("_system.completion", "LOOP_COMPLETE")]),
        );

        let (mut reconciler, _) =
            build_reconciler(config, FakeBroker::new(), FakeStore::new(), runtime);
        let result = reconciler.run().await.unwrap();
        assert!(matches!(result, RunResult::Completed));
    }

    #[tokio::test]
    async fn test_max_iterations_stops_loop() {
        let config = BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 3
hats:
  worker:
    name: Worker
    description: "Loops forever"
    triggers: ["work.start", "work.continue"]
    publishes: ["work.continue"]
    instructions: "Keep going."
"#,
        )
        .unwrap();

        let runtime = FakeRuntime::new();
        runtime.set_response(
            "worker",
            JobResponse::succeed_with_events(vec![("work.continue", "again")]),
        );

        let (mut reconciler, _) =
            build_reconciler(config, FakeBroker::new(), FakeStore::new(), runtime);
        let result = reconciler.run().await.unwrap();
        assert!(matches!(result, RunResult::MaxIterationsReached));
    }

    #[tokio::test]
    async fn test_job_failure_causes_failed_result() {
        // Use a config with a short timeout so the test doesn't hang.
        // After the planner fails on work.start, no more events arrive,
        // and the reconciler times out.
        let config = BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 10
  max_runtime_seconds: 1
hats:
  planner:
    name: Planner
    description: "Plans work"
    triggers: ["work.start"]
    publishes: ["subtask.ready"]
    instructions: "Plan the work."
"#,
        )
        .unwrap();

        let runtime = FakeRuntime::new();
        runtime.set_response("planner", JobResponse::fail("crashed"));

        let (mut reconciler, _) =
            build_reconciler(config, FakeBroker::new(), FakeStore::new(), runtime);
        let result = reconciler.run().await.unwrap();
        // After one failure and no more events, the run times out
        assert!(matches!(
            result,
            RunResult::TimedOut | RunResult::Failed { .. }
        ));
    }

    #[tokio::test]
    async fn test_scratchpad_read_before_job() {
        let config = two_hat_config();
        let runtime = FakeRuntime::new();
        runtime.set_response(
            "planner",
            JobResponse::succeed_with_events(vec![("_system.completion", "LOOP_COMPLETE")]),
        );

        let (mut reconciler, jobs) =
            build_reconciler(config, FakeBroker::new(), FakeStore::new(), runtime);
        reconciler.run().await.unwrap();

        let jobs = jobs.lock().await;
        let planner_job = jobs.iter().find(|j| j.hat_id == "planner").unwrap();
        // When there's no scratchpad in the store, BORING_SCRATCHPAD_CONTENT should not be set
        assert!(!planner_job.env.contains_key("BORING_SCRATCHPAD_CONTENT"));
    }
}
