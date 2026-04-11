use boring_proto::event::Event;

/// Parse agent stdout for emitted events and completion signals.
///
/// Convention:
/// - `BORING_EMIT <topic> <payload...>` → emits an event with that topic and payload
/// - If the completion_promise string appears on any line → emits a system completion event
pub fn parse_output(
    stdout: &str,
    hat_id: &str,
    run_id: &str,
    completion_promise: &str,
    base_sequence: u64,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut seq = base_sequence;
    let mut completion_found = false;

    for line in stdout.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("BORING_EMIT ") {
            // Split into topic and payload (topic is first word, rest is payload)
            let mut parts = rest.splitn(2, ' ');
            if let Some(topic) = parts.next() {
                let payload = parts.next().unwrap_or("");
                if !topic.is_empty() {
                    events.push(Event::new(topic, payload, Some(hat_id), run_id, seq));
                    seq += 1;
                }
            }
        } else if !completion_found && trimmed.contains(completion_promise) {
            completion_found = true;
            events.push(Event::system_completion(run_id, completion_promise, seq));
            seq += 1;
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_emit_event() {
        let stdout = "some output\nBORING_EMIT subtask.ready implement the parser\nmore output\n";
        let events = parse_output(stdout, "planner", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, "subtask.ready");
        assert_eq!(events[0].payload, "implement the parser");
        assert_eq!(events[0].source, Some("planner".to_string()));
        assert_eq!(events[0].run_id, "run-abc");
    }

    #[test]
    fn test_parse_emit_no_payload() {
        let stdout = "BORING_EMIT work.done\n";
        let events = parse_output(stdout, "builder", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, "work.done");
        assert_eq!(events[0].payload, "");
    }

    #[test]
    fn test_parse_completion_promise() {
        let stdout = "did some work\nLOOP_COMPLETE\n";
        let events = parse_output(stdout, "builder", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 1);
        assert!(events[0].is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_parse_completion_inline() {
        let stdout = "all done LOOP_COMPLETE here\n";
        let events = parse_output(stdout, "builder", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 1);
        assert!(events[0].is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_parse_multiple_emits() {
        let stdout = "BORING_EMIT step.one first\nBORING_EMIT step.two second\n";
        let events = parse_output(stdout, "worker", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].topic, "step.one");
        assert_eq!(events[0].payload, "first");
        assert_eq!(events[0].sequence, 0);
        assert_eq!(events[1].topic, "step.two");
        assert_eq!(events[1].payload, "second");
        assert_eq!(events[1].sequence, 1);
    }

    #[test]
    fn test_parse_emit_and_completion() {
        let stdout = "BORING_EMIT subtask.ready go\nLOOP_COMPLETE\n";
        let events = parse_output(stdout, "planner", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].topic, "subtask.ready");
        assert!(events[1].is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_parse_no_events() {
        let stdout = "just regular output\nnothing interesting\n";
        let events = parse_output(stdout, "worker", "run-abc", "LOOP_COMPLETE", 0);

        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_empty_stdout() {
        let events = parse_output("", "worker", "run-abc", "LOOP_COMPLETE", 0);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_completion_only_emitted_once() {
        let stdout = "LOOP_COMPLETE\nmore stuff\nLOOP_COMPLETE again\n";
        let events = parse_output(stdout, "worker", "run-abc", "LOOP_COMPLETE", 0);

        let completions: Vec<_> = events.iter().filter(|e| e.is_completion("LOOP_COMPLETE")).collect();
        assert_eq!(completions.len(), 1);
    }

    #[test]
    fn test_parse_emit_with_leading_whitespace() {
        let stdout = "  BORING_EMIT work.done all finished\n";
        let events = parse_output(stdout, "worker", "run-abc", "LOOP_COMPLETE", 0);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, "work.done");
        assert_eq!(events[0].payload, "all finished");
    }

    #[test]
    fn test_sequence_starts_at_base() {
        let stdout = "BORING_EMIT a first\nBORING_EMIT b second\n";
        let events = parse_output(stdout, "worker", "run-abc", "LOOP_COMPLETE", 42);

        assert_eq!(events[0].sequence, 42);
        assert_eq!(events[1].sequence, 43);
    }
}
