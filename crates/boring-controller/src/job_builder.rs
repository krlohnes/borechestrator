use boring_proto::config::{BoringConfig, EnvValue, HatConfig};
use boring_proto::event::Event;
use boring_runtime::JobSpec;
use boring_secrets::SecretProvider;
use std::collections::HashMap;

/// Builds JobSpecs from hat configs, events, and scratchpad state.
pub struct JobBuilder {
    completion_promise: String,
    guardrails: Vec<String>,
    prompt_file_content: Option<String>,
    backend_command: Option<String>,
    broker_url: Option<String>,
    broker_stream: Option<String>,
    store_endpoint: Option<String>,
    store_bucket: Option<String>,
    store_prefix: Option<String>,
    store_access_key: Option<String>,
    store_secret_key: Option<String>,
    git_repo: Option<String>,
    git_base_branch: Option<String>,
    git_branch_strategy: Option<String>,
    git_credentials_secret: Option<String>,
}

impl JobBuilder {
    pub fn new(config: &BoringConfig) -> Self {
        let guardrails = config
            .core
            .as_ref()
            .map(|c| c.guardrails.clone())
            .unwrap_or_default();

        let prompt_file_content = config
            .event_loop
            .prompt_file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok());

        let backend_command = config.cli.as_ref().map(|c| c.backend_command());

        // Use pod_url for containers if available, otherwise fall back to url
        let (broker_url, broker_stream) = config
            .broker
            .as_ref()
            .map(|b| {
                (
                    Some(b.pod_url.as_ref().unwrap_or(&b.url).clone()),
                    b.stream.clone(),
                )
            })
            .unwrap_or((None, None));

        let (store_endpoint, store_bucket, store_prefix, store_access_key, store_secret_key) =
            config
                .store
                .as_ref()
                .map(|s| {
                    (
                        Some(s.endpoint.clone()),
                        Some(s.bucket.clone()),
                        s.prefix.clone(),
                        None, // access keys come from secrets, not config
                        None,
                    )
                })
                .unwrap_or((None, None, None, None, None));

        let (git_repo, git_base_branch, git_branch_strategy, git_credentials_secret) =
            if let Some(ref git) = config.git {
                (
                    Some(git.repo.clone()),
                    git.base_branch.clone(),
                    git.branch_strategy
                        .as_ref()
                        .map(|s| format!("{:?}", s).to_lowercase()),
                    git.credentials.as_ref().map(|c| c.from_secret.clone()),
                )
            } else {
                (None, None, None, None)
            };

        Self {
            completion_promise: config.event_loop.completion_promise.clone(),
            guardrails,
            prompt_file_content,
            backend_command,
            broker_url,
            broker_stream,
            store_endpoint,
            store_bucket,
            store_prefix,
            store_access_key,
            store_secret_key,
            git_repo,
            git_base_branch,
            git_branch_strategy,
            git_credentials_secret,
        }
    }

    /// Assemble the full prompt from hat instructions, event context, guardrails, and scratchpad.
    pub fn assemble_prompt(
        hat: &HatConfig,
        event: &Event,
        guardrails: &[String],
        scratchpad: Option<&str>,
        prompt_file: Option<&str>,
        completion_promise: &str,
    ) -> String {
        let mut parts = Vec::new();

        // System rules — these are borechestrator's concerns, not the user's.
        // Injected automatically into every hat prompt.
        parts.push(format!(
            "# System\n\n\
            You are running inside borechestrator, an AI agent orchestrator.\n\
            You have full tool access — you can read files, write files, and run shell commands.\n\
            \n\
            ## Workspace\n\
            Your current working directory is a git repository. Write all code files here.\n\
            Do NOT change directories. Create files in the current directory or subdirectories.\n\
            \n\
            - .boring/prompt.md — your task description\n\
            - .boring/event.json — the current event that triggered you\n\
            - .boring/scratchpad/ — shared notes between all hats (read the API contract here)\n\
            - .boring/memories.md — learnings from previous iterations\n\
            - .boring/tasks.md — task list\n\
            \n\
            ## CRITICAL: You MUST run the `emit` command before you finish.\n\
            `emit` is a real CLI tool installed at /usr/local/bin/emit.\n\
            Your allowed events: {publishes}\n\
            \n\
            Run one of these shell commands before finishing:\n\
            ```\n\
            emit {first_publish} \"description of what you did\"\n\
            ```\n\
            Or if the entire orchestration is done:\n\
            ```\n\
            emit --complete\n\
            ```\n\
            \n\
            ## Git\n\
            Before committing: `git pull --rebase origin $(git branch --show-current)`\n\
            Commit and push BEFORE running `emit`.",
            publishes = hat.publishes.join(", "),
            first_publish = hat
                .publishes
                .first()
                .map(|s| s.as_str())
                .unwrap_or("event.done"),
        ));

        // Prompt file (task description, loaded once at run start)
        if let Some(content) = prompt_file {
            parts.push(format!("# Task\n\n{}", content.trim()));
        }

        // Instructions
        parts.push(format!("# Instructions\n\n{}", hat.instructions.trim()));

        // User guardrails (project-specific rules from config)
        if !guardrails.is_empty() {
            let rules: String = guardrails
                .iter()
                .map(|g| format!("- {}", g))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("# Project Rules\n\n{}", rules));
        }

        // Event context
        parts.push(format!(
            "# Current Event\n\nTopic: {}\nPayload: {}",
            event.topic, event.payload
        ));

        // Scratchpad
        if let Some(content) = scratchpad {
            parts.push(format!("# Scratchpad\n\n{}", content));
        }

        parts.join("\n\n---\n\n")
    }

    /// Resolve hat env vars, replacing `from_secret` references with actual values.
    async fn resolve_env(
        hat: &HatConfig,
        secrets: &dyn SecretProvider,
    ) -> anyhow::Result<HashMap<String, String>> {
        let mut resolved = HashMap::new();

        if let Some(ref env_map) = hat.env {
            for (key, value) in env_map {
                match value {
                    EnvValue::Literal(v) => {
                        resolved.insert(key.clone(), v.clone());
                    }
                    EnvValue::FromSecret { from_secret } => {
                        if let Some(secret_value) = secrets.get_secret(from_secret).await? {
                            resolved.insert(key.clone(), secret_value);
                        } else {
                            anyhow::bail!(
                                "secret '{}' not found for env var '{}'",
                                from_secret,
                                key
                            );
                        }
                    }
                }
            }
        }

        Ok(resolved)
    }

    /// Build a complete JobSpec for the given hat activation.
    pub async fn build(
        &self,
        hat_id: &str,
        hat: &HatConfig,
        event: &Event,
        scratchpad: Option<&str>,
        secrets: &dyn SecretProvider,
    ) -> anyhow::Result<JobSpec> {
        let prompt = Self::assemble_prompt(
            hat,
            event,
            &self.guardrails,
            scratchpad,
            self.prompt_file_content.as_deref(),
            &self.completion_promise,
        );

        let mut env = HashMap::new();

        // Ensure emit CLI is on PATH. Look for it next to the boring-cli binary,
        // in /usr/local/bin (containers), or ~/.local/bin (local dev).
        let emit_dirs = [
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string())),
            Some("/usr/local/bin".to_string()),
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{}/.local/bin", h)),
        ];
        let extra_path = emit_dirs
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(":");
        let current_path = std::env::var("PATH").unwrap_or_default();
        env.insert(
            "PATH".to_string(),
            format!("{}:{}", extra_path, current_path),
        );

        env.insert("BORING_RUN_ID".to_string(), event.run_id.clone());
        env.insert("BORING_HAT_ID".to_string(), hat_id.to_string());
        env.insert("BORING_EVENT_TOPIC".to_string(), event.topic.clone());
        env.insert("BORING_EVENT_PAYLOAD".to_string(), event.payload.clone());
        env.insert(
            "BORING_COMPLETION_PROMISE".to_string(),
            self.completion_promise.clone(),
        );
        env.insert("BORING_PROMPT".to_string(), prompt.clone());

        // Default event to emit if the agent doesn't call emit itself
        if let Some(ref dp) = hat.default_publishes {
            env.insert("BORING_DEFAULT_PUBLISH".to_string(), dp.clone());
        } else if let Some(first) = hat.publishes.first() {
            env.insert("BORING_DEFAULT_PUBLISH".to_string(), first.clone());
        }

        // Write prompt to a temp file so backends can read it without shell quoting issues
        let prompt_file =
            std::env::temp_dir().join(format!("boring-prompt-{}-{}.md", event.run_id, hat_id));
        std::fs::write(&prompt_file, &prompt).ok();
        env.insert(
            "BORING_PROMPT_FILE".to_string(),
            prompt_file.to_string_lossy().to_string(),
        );

        if let Some(content) = scratchpad {
            env.insert("BORING_SCRATCHPAD_CONTENT".to_string(), content.to_string());
        }

        // Broker/store config for boring-agent inside containers
        if let Some(ref url) = self.broker_url {
            env.insert("BORING_BROKER_URL".to_string(), url.clone());
        }
        if let Some(ref stream) = self.broker_stream {
            env.insert("BORING_BROKER_STREAM".to_string(), stream.clone());
        }
        if let Some(ref endpoint) = self.store_endpoint {
            env.insert("BORING_STORE_ENDPOINT".to_string(), endpoint.clone());
        }
        if let Some(ref bucket) = self.store_bucket {
            env.insert("BORING_STORE_BUCKET".to_string(), bucket.clone());
        }
        if let Some(ref prefix) = self.store_prefix {
            env.insert("BORING_STORE_PREFIX".to_string(), prefix.clone());
        }

        // Resolve hat-specific env vars (including from_secret)
        let hat_env = Self::resolve_env(hat, secrets).await?;
        env.extend(hat_env);

        // Git config
        if let Some(ref repo) = self.git_repo {
            env.insert("BORING_GIT_REPO".to_string(), repo.clone());
            if let Some(ref branch) = self.git_base_branch {
                env.insert("BORING_GIT_BASE_BRANCH".to_string(), branch.clone());
            }
            if let Some(ref strategy) = self.git_branch_strategy {
                env.insert("BORING_GIT_BRANCH_STRATEGY".to_string(), strategy.clone());
            }
            // Resolve git credentials secret
            if let Some(ref secret_name) = self.git_credentials_secret {
                if let Some(token) = secrets.get_secret(secret_name).await? {
                    env.insert("BORING_GIT_TOKEN".to_string(), token);
                }
            }
        }

        // Priority: hat.command > cli.backend > echo prompt
        let command = hat
            .command
            .clone()
            .or_else(|| self.backend_command.clone())
            .unwrap_or_else(|| "echo \"$BORING_PROMPT\"".to_string());

        let secret_mounts = hat
            .secret_mounts
            .iter()
            .map(|sm| (sm.from_secret.clone(), sm.mount_path.clone()))
            .collect();

        Ok(JobSpec {
            hat_id: hat_id.to_string(),
            run_id: event.run_id.clone(),
            command,
            env,
            working_dir: None,
            secret_mounts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Test secret provider that returns known values.
    struct TestSecrets {
        secrets: HashMap<String, String>,
    }

    impl TestSecrets {
        fn empty() -> Self {
            Self {
                secrets: HashMap::new(),
            }
        }

        fn with(pairs: Vec<(&str, &str)>) -> Self {
            let secrets = pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            Self { secrets }
        }
    }

    #[async_trait]
    impl SecretProvider for TestSecrets {
        async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
            Ok(self.secrets.get(name).cloned())
        }
    }

    fn minimal_config() -> BoringConfig {
        BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do the work."
"#,
        )
        .unwrap()
    }

    fn config_with_guardrails() -> BoringConfig {
        BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
core:
  guardrails:
    - "Always commit after changes."
    - "Run tests before emitting done."
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do the work."
"#,
        )
        .unwrap()
    }

    fn test_event() -> Event {
        Event::new("work.start", "begin the work", None, "run-test", 1)
    }

    #[test]
    fn test_assemble_prompt_includes_instructions() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt =
            JobBuilder::assemble_prompt(hat, &test_event(), &[], None, None, "LOOP_COMPLETE");
        assert!(prompt.contains("Do the work."));
    }

    #[test]
    fn test_assemble_prompt_includes_event_context() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt =
            JobBuilder::assemble_prompt(hat, &test_event(), &[], None, None, "LOOP_COMPLETE");
        assert!(prompt.contains("work.start"));
        assert!(prompt.contains("begin the work"));
    }

    #[test]
    fn test_assemble_prompt_includes_guardrails() {
        let config = config_with_guardrails();
        let hat = &config.hats["worker"];
        let guardrails: Vec<String> = config.core.as_ref().unwrap().guardrails.clone();
        let prompt = JobBuilder::assemble_prompt(
            hat,
            &test_event(),
            &guardrails,
            None,
            None,
            "LOOP_COMPLETE",
        );
        assert!(prompt.contains("Always commit after changes."));
        assert!(prompt.contains("Run tests before emitting done."));
    }

    #[test]
    fn test_assemble_prompt_includes_scratchpad() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt = JobBuilder::assemble_prompt(
            hat,
            &test_event(),
            &[],
            Some("## Progress\n- Step 1 done"),
            None,
            "LOOP_COMPLETE",
        );
        assert!(prompt.contains("## Progress"));
        assert!(prompt.contains("- Step 1 done"));
    }

    #[test]
    fn test_assemble_prompt_no_scratchpad() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt =
            JobBuilder::assemble_prompt(hat, &test_event(), &[], None, None, "LOOP_COMPLETE");
        assert!(!prompt.contains("Scratchpad"));
    }

    #[tokio::test]
    async fn test_build_sets_env_vars() {
        let config = minimal_config();
        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::empty();
        let spec = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await
            .unwrap();

        assert_eq!(spec.env.get("BORING_RUN_ID").unwrap(), "run-test");
        assert_eq!(spec.env.get("BORING_HAT_ID").unwrap(), "worker");
        assert_eq!(spec.env.get("BORING_EVENT_TOPIC").unwrap(), "work.start");
    }

    #[tokio::test]
    async fn test_build_job_spec_hat_id_and_run_id() {
        let config = minimal_config();
        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::empty();
        let spec = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await
            .unwrap();

        assert_eq!(spec.hat_id, "worker");
        assert_eq!(spec.run_id, "run-test");
    }

    #[tokio::test]
    async fn test_build_includes_completion_promise_in_env() {
        let config = minimal_config();
        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::empty();
        let spec = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await
            .unwrap();

        assert_eq!(
            spec.env.get("BORING_COMPLETION_PROMISE").unwrap(),
            "LOOP_COMPLETE"
        );
    }

    #[tokio::test]
    async fn test_build_resolves_literal_env() {
        let config = BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
    env:
      DEBUG: "true"
      VERBOSE: "1"
"#,
        )
        .unwrap();

        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::empty();
        let spec = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await
            .unwrap();

        assert_eq!(spec.env.get("DEBUG").unwrap(), "true");
        assert_eq!(spec.env.get("VERBOSE").unwrap(), "1");
    }

    #[tokio::test]
    async fn test_build_resolves_from_secret() {
        let config = BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
    env:
      ANTHROPIC_API_KEY:
        from_secret: anthropic-api-key
"#,
        )
        .unwrap();

        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::with(vec![("anthropic-api-key", "sk-ant-12345")]);
        let spec = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await
            .unwrap();

        assert_eq!(spec.env.get("ANTHROPIC_API_KEY").unwrap(), "sk-ant-12345");
    }

    #[tokio::test]
    async fn test_build_fails_on_missing_secret() {
        let config = BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
    env:
      API_KEY:
        from_secret: nonexistent-secret
"#,
        )
        .unwrap();

        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::empty();
        let result = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent-secret"));
    }

    #[tokio::test]
    async fn test_build_mixed_literal_and_secret_env() {
        let config = BoringConfig::from_yaml(
            r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
hats:
  worker:
    name: Worker
    description: "Does work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
    env:
      DEBUG: "true"
      API_KEY:
        from_secret: my-key
"#,
        )
        .unwrap();

        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let secrets = TestSecrets::with(vec![("my-key", "secret-value")]);
        let spec = builder
            .build("worker", hat, &test_event(), None, &secrets)
            .await
            .unwrap();

        assert_eq!(spec.env.get("DEBUG").unwrap(), "true");
        assert_eq!(spec.env.get("API_KEY").unwrap(), "secret-value");
    }
}
