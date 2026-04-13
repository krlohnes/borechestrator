use crate::human::HumanAction;
use crate::memories::Memory;
use crate::tasks::{Task, TaskAction, TaskStatus};
use boring_proto::event::Event;

/// Everything parsed from agent stdout.
#[derive(Debug, Default)]
pub struct ParsedOutput {
    pub events: Vec<Event>,
    pub memories: Vec<Memory>,
    pub task_actions: Vec<TaskAction>,
    pub human_actions: Vec<HumanAction>,
    pub scratchpad_lines: Vec<String>,
}

/// Parse agent stdout for all borechestrator markers.
///
/// Conventions:
/// - `BORING_EMIT <topic> <payload...>` → event
/// - `BORING_MEMORY <type> <content>` → memory
/// - `BORING_TASK <add|done|progress> <arg>` → task action
/// - `BORING_HUMAN <question>` → human interaction (ask)
/// - `BORING_NOTIFY <message>` → human interaction (notify)
/// - `BORING_SCRATCHPAD <content>` → scratchpad append
/// - completion_promise string on any line → system completion event
pub fn parse_output(
    stdout: &str,
    hat_id: &str,
    run_id: &str,
    completion_promise: &str,
    base_sequence: u64,
) -> ParsedOutput {
    let mut result = ParsedOutput::default();
    let mut seq = base_sequence;
    let mut completion_found = false;

    for line in stdout.lines() {
        let trimmed = line.trim();

        // Use contains() instead of strip_prefix() — LLMs wrap markers in
        // markdown bold, backticks, list items, etc. We don't care where
        // the marker appears, just that it's there.
        if let Some(pos) = trimmed.find("BORING_EMIT ") {
            let rest = &trimmed[pos + "BORING_EMIT ".len()..];
            let rest = rest
                .trim_end_matches(|c: char| c == '*' || c == '`' || c == '_')
                .trim();
            let mut parts = rest.splitn(2, ' ');
            if let Some(topic) = parts.next() {
                let topic = topic.trim_matches(|c: char| c == '*' || c == '`' || c == '_');
                let payload = parts
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(|c: char| c == '*' || c == '`');
                if !topic.is_empty() {
                    result
                        .events
                        .push(Event::new(topic, payload, Some(hat_id), run_id, seq));
                    seq += 1;
                }
            }
        } else if let Some(pos) = trimmed.find("BORING_MEMORY ") {
            let rest = &trimmed[pos + "BORING_MEMORY ".len()..];
            let rest = rest.trim_end_matches(|c: char| c == '*' || c == '`').trim();
            let mut parts = rest.splitn(2, ' ');
            if let Some(memory_type) = parts.next() {
                let content = parts.next().unwrap_or("");
                if !memory_type.is_empty() {
                    result.memories.push(Memory {
                        memory_type: memory_type.to_string(),
                        content: content.to_string(),
                        source: hat_id.to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }
            }
        } else if let Some(pos) = trimmed.find("BORING_TASK ") {
            let rest = &trimmed[pos + "BORING_TASK ".len()..];
            let mut parts = rest.splitn(2, ' ');
            if let Some(action) = parts.next() {
                let arg = parts.next().unwrap_or("");
                match action {
                    "add" => {
                        result.task_actions.push(TaskAction::Add(Task {
                            id: format!("task-{}", seq),
                            title: arg.to_string(),
                            status: TaskStatus::Pending,
                            priority: None,
                            assigned_to: None,
                            depends_on: Vec::new(),
                            created_by: hat_id.to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        }));
                    }
                    "done" => {
                        result.task_actions.push(TaskAction::Done(arg.to_string()));
                    }
                    "progress" => {
                        result
                            .task_actions
                            .push(TaskAction::InProgress(arg.to_string()));
                    }
                    _ => {}
                }
            }
        } else if let Some(pos) = trimmed.find("BORING_HUMAN ") {
            let question = &trimmed[pos + "BORING_HUMAN ".len()..];
            result.human_actions.push(HumanAction::Ask(
                question
                    .trim_end_matches(|c: char| c == '*' || c == '`')
                    .to_string(),
            ));
        } else if let Some(pos) = trimmed.find("BORING_NOTIFY ") {
            let message = &trimmed[pos + "BORING_NOTIFY ".len()..];
            result.human_actions.push(HumanAction::Notify(
                message
                    .trim_end_matches(|c: char| c == '*' || c == '`')
                    .to_string(),
            ));
        } else if let Some(pos) = trimmed.find("BORING_SCRATCHPAD ") {
            let content = &trimmed[pos + "BORING_SCRATCHPAD ".len()..];
            result.scratchpad_lines.push(content.to_string());
        } else if !completion_found && {
            // Completion promise must be the entire line (with optional markdown/whitespace).
            // This prevents false positives from lines like "When done, print LOOP_COMPLETE".
            let stripped = trimmed.trim_matches(|c: char| {
                c == '*' || c == '`' || c == '_' || c == '#' || c == '-' || c.is_whitespace()
            });
            stripped == completion_promise
        } {
            completion_found = true;
            result
                .events
                .push(Event::system_completion(run_id, completion_promise, seq));
            seq += 1;
        }
    }

    result
}

/// Convenience: extract just events (backward compat for reconciler).
pub fn parse_events(
    stdout: &str,
    hat_id: &str,
    run_id: &str,
    completion_promise: &str,
    base_sequence: u64,
) -> Vec<Event> {
    parse_output(stdout, hat_id, run_id, completion_promise, base_sequence).events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_emit_event() {
        let stdout = "some output\nBORING_EMIT subtask.ready implement the parser\nmore output\n";
        let result = parse_output(stdout, "planner", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].topic, "subtask.ready");
        assert_eq!(result.events[0].payload, "implement the parser");
        assert_eq!(result.events[0].source, Some("planner".to_string()));
    }

    #[test]
    fn test_parse_emit_no_payload() {
        let result = parse_output(
            "BORING_EMIT work.done\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].topic, "work.done");
        assert_eq!(result.events[0].payload, "");
    }

    #[test]
    fn test_parse_completion_promise() {
        let result = parse_output(
            "did some work\nLOOP_COMPLETE\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.events.len(), 1);
        assert!(result.events[0].is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_parse_completion_inline_does_not_match() {
        // Completion must be the entire line, not embedded in text.
        // Prevents false positives from LLM output like "print LOOP_COMPLETE when done".
        let result = parse_output(
            "all done LOOP_COMPLETE here\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.events.len(), 0);
    }

    #[test]
    fn test_parse_multiple_emits() {
        let result = parse_output(
            "BORING_EMIT step.one first\nBORING_EMIT step.two second\n",
            "worker",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.events[0].topic, "step.one");
        assert_eq!(result.events[1].topic, "step.two");
        assert_eq!(result.events[0].sequence, 0);
        assert_eq!(result.events[1].sequence, 1);
    }

    #[test]
    fn test_parse_emit_and_completion() {
        let result = parse_output(
            "BORING_EMIT subtask.ready go\nLOOP_COMPLETE\n",
            "planner",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.events[0].topic, "subtask.ready");
        assert!(result.events[1].is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_parse_no_events() {
        let result = parse_output(
            "just regular output\n",
            "worker",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert!(result.events.is_empty());
    }

    #[test]
    fn test_parse_empty_stdout() {
        let result = parse_output("", "worker", "run-abc", "LOOP_COMPLETE", 0);
        assert!(result.events.is_empty());
    }

    #[test]
    fn test_parse_completion_only_emitted_once() {
        let result = parse_output(
            "LOOP_COMPLETE\nmore\nLOOP_COMPLETE again\n",
            "worker",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        let completions: Vec<_> = result
            .events
            .iter()
            .filter(|e| e.is_completion("LOOP_COMPLETE"))
            .collect();
        assert_eq!(completions.len(), 1);
    }

    #[test]
    fn test_parse_emit_with_leading_whitespace() {
        let result = parse_output(
            "  BORING_EMIT work.done all finished\n",
            "worker",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].payload, "all finished");
    }

    #[test]
    fn test_sequence_starts_at_base() {
        let result = parse_output(
            "BORING_EMIT a first\nBORING_EMIT b second\n",
            "worker",
            "run-abc",
            "LOOP_COMPLETE",
            42,
        );
        assert_eq!(result.events[0].sequence, 42);
        assert_eq!(result.events[1].sequence, 43);
    }

    // ── New marker tests ────────────────────────────────────────

    #[test]
    fn test_parse_memory() {
        let result = parse_output(
            "BORING_MEMORY pattern Always use snake_case\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.memories.len(), 1);
        assert_eq!(result.memories[0].memory_type, "pattern");
        assert_eq!(result.memories[0].content, "Always use snake_case");
        assert_eq!(result.memories[0].source, "builder");
    }

    #[test]
    fn test_parse_task_add() {
        let result = parse_output(
            "BORING_TASK add Implement user auth\n",
            "planner",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.task_actions.len(), 1);
        match &result.task_actions[0] {
            TaskAction::Add(task) => {
                assert_eq!(task.title, "Implement user auth");
                assert_eq!(task.created_by, "planner");
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_task_done() {
        let result = parse_output(
            "BORING_TASK done task-42\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.task_actions.len(), 1);
        match &result.task_actions[0] {
            TaskAction::Done(id) => assert_eq!(id, "task-42"),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_parse_human_ask() {
        let result = parse_output(
            "BORING_HUMAN Should I proceed with the migration?\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.human_actions.len(), 1);
        match &result.human_actions[0] {
            HumanAction::Ask(q) => assert_eq!(q, "Should I proceed with the migration?"),
            _ => panic!("expected Ask"),
        }
    }

    #[test]
    fn test_parse_notify() {
        let result = parse_output(
            "BORING_NOTIFY Build completed\n",
            "builder",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.human_actions.len(), 1);
        match &result.human_actions[0] {
            HumanAction::Notify(m) => assert_eq!(m, "Build completed"),
            _ => panic!("expected Notify"),
        }
    }

    #[test]
    fn test_parse_scratchpad() {
        let result = parse_output(
            "BORING_SCRATCHPAD Step 3 done\nBORING_SCRATCHPAD Moving to step 4\n",
            "worker",
            "run-abc",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(result.scratchpad_lines.len(), 2);
        assert_eq!(result.scratchpad_lines[0], "Step 3 done");
        assert_eq!(result.scratchpad_lines[1], "Moving to step 4");
    }

    #[test]
    fn test_parse_all_markers_mixed() {
        let stdout = "\
BORING_EMIT subtask.ready go
BORING_MEMORY decision Chose async approach
BORING_TASK add Write tests
BORING_HUMAN Approve this change?
BORING_NOTIFY Starting build
BORING_SCRATCHPAD Progress updated
LOOP_COMPLETE
";
        let result = parse_output(stdout, "planner", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(result.events.len(), 2); // emit + completion
        assert_eq!(result.memories.len(), 1);
        assert_eq!(result.task_actions.len(), 1);
        assert_eq!(result.human_actions.len(), 2); // ask + notify
        assert_eq!(result.scratchpad_lines.len(), 1);
    }

    #[test]
    fn test_parse_events_compat() {
        let events = parse_events(
            "BORING_EMIT work.done\nLOOP_COMPLETE\n",
            "w",
            "r",
            "LOOP_COMPLETE",
            0,
        );
        assert_eq!(events.len(), 2);
    }
}
