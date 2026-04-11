use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A typed message flowing between hats through the broker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub topic: String,
    pub payload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub run_id: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
}

impl Event {
    /// Create a new event with the current timestamp.
    pub fn new(
        topic: &str,
        payload: &str,
        source: Option<&str>,
        run_id: &str,
        sequence: u64,
    ) -> Self {
        Self {
            topic: topic.to_string(),
            payload: payload.to_string(),
            source: source.map(|s| s.to_string()),
            target: None,
            run_id: run_id.to_string(),
            sequence,
            timestamp: Utc::now(),
        }
    }

    /// Create a system completion event.
    pub fn system_completion(run_id: &str, promise: &str, sequence: u64) -> Self {
        Self {
            topic: "_system.completion".to_string(),
            payload: promise.to_string(),
            source: None,
            target: None,
            run_id: run_id.to_string(),
            sequence,
            timestamp: Utc::now(),
        }
    }

    /// Check if this event signals completion with the given promise string.
    pub fn is_completion(&self, promise: &str) -> bool {
        self.topic == "_system.completion" && self.payload == promise
    }

    /// Check if this is a system event (topic starts with `_system.`).
    pub fn is_system(&self) -> bool {
        self.topic.starts_with("_system.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_event() {
        let event = Event::new("work.start", "begin", Some("planner"), "run-abc", 1);
        assert_eq!(event.topic, "work.start");
        assert_eq!(event.payload, "begin");
        assert_eq!(event.source, Some("planner".to_string()));
        assert_eq!(event.target, None);
        assert_eq!(event.run_id, "run-abc");
        assert_eq!(event.sequence, 1);
    }

    #[test]
    fn test_new_event_no_source() {
        let event = Event::new("work.start", "begin", None, "run-abc", 0);
        assert_eq!(event.source, None);
    }

    #[test]
    fn test_system_completion() {
        let event = Event::system_completion("run-abc", "LOOP_COMPLETE", 42);
        assert_eq!(event.topic, "_system.completion");
        assert_eq!(event.payload, "LOOP_COMPLETE");
        assert_eq!(event.source, None);
        assert_eq!(event.run_id, "run-abc");
        assert_eq!(event.sequence, 42);
    }

    #[test]
    fn test_is_completion_true() {
        let event = Event::system_completion("run-abc", "LOOP_COMPLETE", 1);
        assert!(event.is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_is_completion_wrong_promise() {
        let event = Event::system_completion("run-abc", "LOOP_COMPLETE", 1);
        assert!(!event.is_completion("OTHER_PROMISE"));
    }

    #[test]
    fn test_is_completion_normal_event() {
        let event = Event::new("work.start", "LOOP_COMPLETE", None, "run-abc", 1);
        assert!(!event.is_completion("LOOP_COMPLETE"));
    }

    #[test]
    fn test_is_system() {
        let sys = Event::system_completion("run-abc", "LOOP_COMPLETE", 1);
        let normal = Event::new("work.start", "begin", None, "run-abc", 1);
        assert!(sys.is_system());
        assert!(!normal.is_system());
    }

    #[test]
    fn test_json_roundtrip() {
        let event = Event::new("subtask.ready", "implement parser", Some("planner"), "run-abc", 5);
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic, event.topic);
        assert_eq!(deserialized.payload, event.payload);
        assert_eq!(deserialized.source, event.source);
        assert_eq!(deserialized.target, event.target);
        assert_eq!(deserialized.run_id, event.run_id);
        assert_eq!(deserialized.sequence, event.sequence);
    }

    #[test]
    fn test_json_with_target() {
        let mut event = Event::new("subtask.ready", "task", Some("planner"), "run-abc", 1);
        event.target = Some("builder".to_string());
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.target, Some("builder".to_string()));
    }

    #[test]
    fn test_json_omits_none_fields() {
        let event = Event::new("work.start", "begin", None, "run-abc", 1);
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("\"source\""));
        assert!(!json.contains("\"target\""));
    }
}
