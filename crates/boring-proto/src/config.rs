use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

use crate::topic::Topic;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("validation error: {0}")]
    Validation(String),
}

/// Top-level borechestrator configuration.
#[derive(Debug, Deserialize)]
pub struct BoringConfig {
    pub event_loop: EventLoopConfig,
    /// Ralph-compatible CLI backend config.
    pub cli: Option<CliConfig>,
    pub runtime: Option<RuntimeConfig>,
    pub store: Option<StoreConfig>,
    pub broker: Option<BrokerConfig>,
    pub git: Option<GitConfig>,
    pub backpressure: Option<BackpressureConfig>,
    pub memories: Option<MemoriesConfig>,
    pub tasks: Option<TasksConfig>,
    pub hooks: Option<HooksConfig>,
    pub core: Option<CoreConfig>,
    pub hats: HashMap<String, HatConfig>,
}

impl BoringConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self, ConfigError> {
        let config: BoringConfig = serde_yaml::from_str(yaml)?;
        if config.hats.is_empty() {
            return Err(ConfigError::Validation("hats map must not be empty".to_string()));
        }
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_yaml(&contents)
    }

    pub fn validate(&self) -> Result<(), Vec<ConfigError>> {
        let mut errors = Vec::new();

        // Check each hat has non-empty triggers
        for (id, hat) in &self.hats {
            if hat.triggers.is_empty() {
                errors.push(ConfigError::Validation(format!(
                    "hat '{}' has empty triggers",
                    id
                )));
            }
        }

        // Check at least one hat matches the starting_event
        let starting = &self.event_loop.starting_event;
        let has_match = self.hats.values().any(|hat| {
            hat.triggers.iter().any(|trigger| {
                let topic = Topic::new(trigger);
                topic.matches(starting)
            })
        });
        if !has_match {
            errors.push(ConfigError::Validation(format!(
                "no hat has a trigger matching starting_event '{}'",
                starting
            )));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EventLoopConfig {
    pub starting_event: String,
    pub completion_promise: String,
    pub max_iterations: Option<u32>,
    pub max_runtime_seconds: Option<u64>,
    /// Path to a prompt file (e.g., PROMPT.md) loaded at run start.
    pub prompt_file: Option<String>,
    /// Events that must have been seen before completion is allowed.
    pub required_events: Option<Vec<String>>,
    /// How often to checkpoint run state (every N iterations).
    pub checkpoint_interval: Option<u32>,
}

/// CLI backend config (Ralph-compatible).
#[derive(Debug, Deserialize)]
pub struct CliConfig {
    /// AI backend: claude, kiro, gemini, codex, amp, copilot, opencode, custom
    pub backend: String,
    /// How to pass the prompt: "arg" (default) or "stdin"
    #[serde(default = "default_prompt_mode")]
    pub prompt_mode: String,
}

fn default_prompt_mode() -> String {
    "arg".to_string()
}

impl CliConfig {
    /// Get the shell command template for the configured backend.
    /// Uses $BORING_PROMPT_FILE to avoid shell quoting issues with large prompts.
    pub fn backend_command(&self) -> String {
        match self.backend.as_str() {
            "claude" => "claude --print -p \"$(cat $BORING_PROMPT_FILE)\"".to_string(),
            "kiro" => "kiro --print -p \"$(cat $BORING_PROMPT_FILE)\"".to_string(),
            "gemini" => "gemini < $BORING_PROMPT_FILE".to_string(),
            "codex" => "codex < $BORING_PROMPT_FILE".to_string(),
            "amp" => "amp < $BORING_PROMPT_FILE".to_string(),
            "copilot" => "copilot < $BORING_PROMPT_FILE".to_string(),
            "opencode" => "opencode < $BORING_PROMPT_FILE".to_string(),
            other => format!("{} < $BORING_PROMPT_FILE", other),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    Local,
    Docker,
    K8s,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
    pub namespace: Option<String>,
    pub default_image: Option<String>,
    pub image_pull_policy: Option<String>,
    pub resources: Option<Resources>,
    pub job_timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resources {
    pub requests: Option<HashMap<String, String>>,
    pub limits: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct StoreConfig {
    pub endpoint: String,
    pub bucket: String,
    pub prefix: Option<String>,
    pub region: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BrokerConfig {
    pub url: String,
    pub stream: Option<String>,
    pub credentials_file: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BranchStrategy {
    Shared,
    PerHat,
}

#[derive(Debug, Deserialize)]
pub struct GitConfig {
    pub repo: String,
    pub base_branch: Option<String>,
    pub branch_strategy: Option<BranchStrategy>,
    pub credentials: Option<GitCredentials>,
}

#[derive(Debug, Deserialize)]
pub struct GitCredentials {
    pub from_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct CoreConfig {
    #[serde(default)]
    pub guardrails: Vec<String>,
}

/// Memories config for cross-iteration learning.
#[derive(Debug, Deserialize)]
pub struct MemoriesConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// auto, manual, or none
    #[serde(default = "default_inject")]
    pub inject: String,
    /// Max tokens to inject into prompts
    #[serde(default = "default_budget")]
    pub budget: usize,
}

fn default_true() -> bool { true }
fn default_inject() -> String { "auto".to_string() }
fn default_budget() -> usize { 2000 }

/// Tasks config for work item tracking.
#[derive(Debug, Deserialize)]
pub struct TasksConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Hooks config for lifecycle events.
#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub events: std::collections::HashMap<String, Vec<HookDef>>,
}

#[derive(Debug, Deserialize)]
pub struct HookDef {
    pub name: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub on_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BackpressureConfig {
    #[serde(default)]
    pub gates: Vec<Gate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Gate {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub on_fail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HatConfig {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub publishes: Vec<String>,
    pub default_publishes: Option<String>,
    pub instructions: String,
    /// Shell command to run in local mode. The prompt is available as $BORING_PROMPT.
    pub command: Option<String>,
    pub image: Option<String>,
    pub env: Option<HashMap<String, EnvValue>>,
    pub resources: Option<Resources>,
    pub max_activations: Option<u32>,
    pub concurrency: Option<u32>,
    /// Per-hat gates that must pass before the hat's command runs.
    #[serde(default)]
    pub gates: Vec<Gate>,
    /// Secrets mounted as files inside the container.
    #[serde(default)]
    pub secret_mounts: Vec<SecretMount>,
}

/// A secret mounted as a file in the container.
#[derive(Debug, Clone, Deserialize)]
pub struct SecretMount {
    /// Secret name (resolved via SecretProvider or K8s Secret name)
    pub from_secret: String,
    /// Path inside the container where the secret is mounted
    pub mount_path: String,
}

/// Environment variable value: either a literal string or a reference to a K8s secret.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Literal(String),
    FromSecret { from_secret: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_CONFIG: &str = r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE

hats:
  worker:
    name: Worker
    description: "Does the work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do the work."
"#;

    const FULL_CONFIG: &str = r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
  max_iterations: 50
  max_runtime_seconds: 3600

runtime:
  mode: local
  namespace: default
  default_image: ghcr.io/org/agent:latest
  job_timeout_seconds: 300

store:
  endpoint: http://minio:9000
  bucket: borechestrator
  prefix: my-project
  region: us-east-1

broker:
  url: nats://nats:4222
  stream: BORING

git:
  repo: https://github.com/org/project.git
  base_branch: main
  branch_strategy: shared

core:
  guardrails:
    - "Rule one"
    - "Rule two"

hats:
  planner:
    name: Planner
    description: "Plans work"
    triggers: ["work.start"]
    publishes: ["subtask.ready"]
    default_publishes: subtask.ready
    max_activations: 10
    instructions: "Plan the work."
  builder:
    name: Builder
    description: "Builds things"
    triggers: ["subtask.ready"]
    publishes: ["work.done"]
    image: ghcr.io/org/builder:latest
    instructions: "Build it."
"#;

    #[test]
    fn test_parse_minimal_config() {
        let config = BoringConfig::from_yaml(MINIMAL_CONFIG).unwrap();
        assert_eq!(config.event_loop.starting_event, "work.start");
        assert_eq!(config.event_loop.completion_promise, "LOOP_COMPLETE");
        assert_eq!(config.event_loop.max_iterations, None);
        assert_eq!(config.hats.len(), 1);
        assert!(config.hats.contains_key("worker"));
    }

    #[test]
    fn test_parse_full_config() {
        let config = BoringConfig::from_yaml(FULL_CONFIG).unwrap();
        assert_eq!(config.event_loop.max_iterations, Some(50));
        assert_eq!(config.event_loop.max_runtime_seconds, Some(3600));
        assert_eq!(config.hats.len(), 2);

        let planner = &config.hats["planner"];
        assert_eq!(planner.name, "Planner");
        assert_eq!(planner.triggers, vec!["work.start"]);
        assert_eq!(planner.publishes, vec!["subtask.ready"]);
        assert_eq!(planner.default_publishes, Some("subtask.ready".to_string()));
        assert_eq!(planner.max_activations, Some(10));

        let builder = &config.hats["builder"];
        assert_eq!(builder.image, Some("ghcr.io/org/builder:latest".to_string()));
    }

    #[test]
    fn test_parse_runtime_config() {
        let config = BoringConfig::from_yaml(FULL_CONFIG).unwrap();
        let runtime = config.runtime.unwrap();
        assert_eq!(runtime.mode, RuntimeMode::Local);
        assert_eq!(runtime.namespace, Some("default".to_string()));
        assert_eq!(runtime.default_image, Some("ghcr.io/org/agent:latest".to_string()));
        assert_eq!(runtime.job_timeout_seconds, Some(300));
    }

    #[test]
    fn test_parse_store_config() {
        let config = BoringConfig::from_yaml(FULL_CONFIG).unwrap();
        let store = config.store.unwrap();
        assert_eq!(store.endpoint, "http://minio:9000");
        assert_eq!(store.bucket, "borechestrator");
        assert_eq!(store.prefix, Some("my-project".to_string()));
        assert_eq!(store.region, Some("us-east-1".to_string()));
    }

    #[test]
    fn test_parse_broker_config() {
        let config = BoringConfig::from_yaml(FULL_CONFIG).unwrap();
        let broker = config.broker.unwrap();
        assert_eq!(broker.url, "nats://nats:4222");
        assert_eq!(broker.stream, Some("BORING".to_string()));
    }

    #[test]
    fn test_parse_git_config() {
        let config = BoringConfig::from_yaml(FULL_CONFIG).unwrap();
        let git = config.git.unwrap();
        assert_eq!(git.repo, "https://github.com/org/project.git");
        assert_eq!(git.base_branch, Some("main".to_string()));
        assert_eq!(git.branch_strategy, Some(BranchStrategy::Shared));
    }

    #[test]
    fn test_parse_guardrails() {
        let config = BoringConfig::from_yaml(FULL_CONFIG).unwrap();
        let core = config.core.unwrap();
        assert_eq!(core.guardrails.len(), 2);
        assert_eq!(core.guardrails[0], "Rule one");
    }

    #[test]
    fn test_missing_event_loop_fails() {
        let yaml = r#"
hats:
  worker:
    name: Worker
    description: "Does the work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
"#;
        let result = BoringConfig::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_hats_fails() {
        let yaml = r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE
"#;
        let result = BoringConfig::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_hat_with_empty_triggers() {
        let yaml = r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE

hats:
  worker:
    name: Worker
    description: "Does the work"
    triggers: []
    publishes: ["work.done"]
    instructions: "Do it."
"#;
        let config = BoringConfig::from_yaml(yaml).unwrap();
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| format!("{}", e).contains("triggers")));
    }

    #[test]
    fn test_validate_no_hat_matches_starting_event() {
        let yaml = r#"
event_loop:
  starting_event: nonexistent.event
  completion_promise: LOOP_COMPLETE

hats:
  worker:
    name: Worker
    description: "Does the work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
"#;
        let config = BoringConfig::from_yaml(yaml).unwrap();
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| format!("{}", e).contains("starting_event")));
    }

    #[test]
    fn test_validate_valid_config_passes() {
        let config = BoringConfig::from_yaml(MINIMAL_CONFIG).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_hat_env_literal_value() {
        let yaml = r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE

hats:
  worker:
    name: Worker
    description: "Does the work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
    env:
      DEBUG: "true"
"#;
        let config = BoringConfig::from_yaml(yaml).unwrap();
        let env = config.hats["worker"].env.as_ref().unwrap();
        match &env["DEBUG"] {
            EnvValue::Literal(v) => assert_eq!(v, "true"),
            _ => panic!("expected literal env value"),
        }
    }

    #[test]
    fn test_hat_env_from_secret() {
        let yaml = r#"
event_loop:
  starting_event: work.start
  completion_promise: LOOP_COMPLETE

hats:
  worker:
    name: Worker
    description: "Does the work"
    triggers: ["work.start"]
    publishes: ["work.done"]
    instructions: "Do it."
    env:
      API_KEY:
        from_secret: my-secret
"#;
        let config = BoringConfig::from_yaml(yaml).unwrap();
        let env = config.hats["worker"].env.as_ref().unwrap();
        match &env["API_KEY"] {
            EnvValue::FromSecret { from_secret } => assert_eq!(from_secret, "my-secret"),
            _ => panic!("expected from_secret env value"),
        }
    }
}
