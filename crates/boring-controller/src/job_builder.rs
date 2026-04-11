use std::collections::HashMap;
use boring_proto::config::{BoringConfig, HatConfig};
use boring_proto::event::Event;
use boring_runtime::JobSpec;

/// Builds JobSpecs from hat configs, events, and scratchpad state.
pub struct JobBuilder {
    completion_promise: String,
    guardrails: Vec<String>,
}

impl JobBuilder {
    pub fn new(config: &BoringConfig) -> Self {
        let guardrails = config
            .core
            .as_ref()
            .map(|c| c.guardrails.clone())
            .unwrap_or_default();

        Self {
            completion_promise: config.event_loop.completion_promise.clone(),
            guardrails,
        }
    }

    /// Assemble the full prompt from hat instructions, event context, guardrails, and scratchpad.
    pub fn assemble_prompt(
        hat: &HatConfig,
        event: &Event,
        guardrails: &[String],
        scratchpad: Option<&str>,
    ) -> String {
        let mut parts = Vec::new();

        // Instructions
        parts.push(format!("# Instructions\n\n{}", hat.instructions.trim()));

        // Guardrails
        if !guardrails.is_empty() {
            let rules: String = guardrails
                .iter()
                .map(|g| format!("- {}", g))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("# Guardrails\n\n{}", rules));
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

    /// Build a complete JobSpec for the given hat activation.
    pub fn build(
        &self,
        hat_id: &str,
        hat: &HatConfig,
        event: &Event,
        scratchpad: Option<&str>,
    ) -> JobSpec {
        let prompt = Self::assemble_prompt(hat, event, &self.guardrails, scratchpad);

        let mut env = HashMap::new();
        env.insert("BORING_RUN_ID".to_string(), event.run_id.clone());
        env.insert("BORING_HAT_ID".to_string(), hat_id.to_string());
        env.insert("BORING_EVENT_TOPIC".to_string(), event.topic.clone());
        env.insert("BORING_EVENT_PAYLOAD".to_string(), event.payload.clone());
        env.insert("BORING_COMPLETION_PROMISE".to_string(), self.completion_promise.clone());
        env.insert("BORING_PROMPT".to_string(), prompt.clone());

        if let Some(content) = scratchpad {
            env.insert("BORING_SCRATCHPAD_CONTENT".to_string(), content.to_string());
        }

        JobSpec {
            hat_id: hat_id.to_string(),
            run_id: event.run_id.clone(),
            command: prompt,
            env,
            working_dir: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_config() -> BoringConfig {
        BoringConfig::from_yaml(r#"
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
"#).unwrap()
    }

    fn config_with_guardrails() -> BoringConfig {
        BoringConfig::from_yaml(r#"
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
"#).unwrap()
    }

    fn test_event() -> Event {
        Event::new("work.start", "begin the work", None, "run-test", 1)
    }

    #[test]
    fn test_assemble_prompt_includes_instructions() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt = JobBuilder::assemble_prompt(hat, &test_event(), &[], None);
        assert!(prompt.contains("Do the work."));
    }

    #[test]
    fn test_assemble_prompt_includes_event_context() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt = JobBuilder::assemble_prompt(hat, &test_event(), &[], None);
        assert!(prompt.contains("work.start"));
        assert!(prompt.contains("begin the work"));
    }

    #[test]
    fn test_assemble_prompt_includes_guardrails() {
        let config = config_with_guardrails();
        let hat = &config.hats["worker"];
        let guardrails: Vec<String> = config.core.as_ref().unwrap().guardrails.clone();
        let prompt = JobBuilder::assemble_prompt(hat, &test_event(), &guardrails, None);
        assert!(prompt.contains("Always commit after changes."));
        assert!(prompt.contains("Run tests before emitting done."));
    }

    #[test]
    fn test_assemble_prompt_includes_scratchpad() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt = JobBuilder::assemble_prompt(hat, &test_event(), &[], Some("## Progress\n- Step 1 done"));
        assert!(prompt.contains("## Progress"));
        assert!(prompt.contains("- Step 1 done"));
    }

    #[test]
    fn test_assemble_prompt_no_scratchpad() {
        let config = minimal_config();
        let hat = &config.hats["worker"];
        let prompt = JobBuilder::assemble_prompt(hat, &test_event(), &[], None);
        assert!(!prompt.contains("Scratchpad"));
    }

    #[test]
    fn test_build_sets_env_vars() {
        let config = minimal_config();
        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let spec = builder.build("worker", hat, &test_event(), None);

        assert_eq!(spec.env.get("BORING_RUN_ID").unwrap(), "run-test");
        assert_eq!(spec.env.get("BORING_HAT_ID").unwrap(), "worker");
        assert_eq!(spec.env.get("BORING_EVENT_TOPIC").unwrap(), "work.start");
    }

    #[test]
    fn test_build_job_spec_hat_id_and_run_id() {
        let config = minimal_config();
        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let spec = builder.build("worker", hat, &test_event(), None);

        assert_eq!(spec.hat_id, "worker");
        assert_eq!(spec.run_id, "run-test");
    }

    #[test]
    fn test_build_includes_completion_promise_in_env() {
        let config = minimal_config();
        let builder = JobBuilder::new(&config);
        let hat = &config.hats["worker"];
        let spec = builder.build("worker", hat, &test_event(), None);

        assert_eq!(spec.env.get("BORING_COMPLETION_PROMISE").unwrap(), "LOOP_COMPLETE");
    }
}
