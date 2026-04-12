use std::collections::HashMap;
use boring_proto::config::HatConfig;
use boring_proto::event::Event;
use boring_proto::topic::Topic;

/// Routes events to hats based on their trigger patterns.
pub struct EventRouter {
    /// Hat configs indexed by hat_id.
    hats: HashMap<String, HatConfig>,
    /// Pre-compiled trigger patterns per hat: (hat_id, [(trigger_pattern, Topic)]).
    triggers: Vec<(String, Vec<(String, Topic)>)>,
}

impl EventRouter {
    pub fn new(hats: HashMap<String, HatConfig>) -> Self {
        let triggers = hats
            .iter()
            .map(|(id, hat)| {
                let patterns: Vec<(String, Topic)> = hat
                    .triggers
                    .iter()
                    .map(|t| (t.clone(), Topic::new(t)))
                    .collect();
                (id.clone(), patterns)
            })
            .collect();

        Self { hats, triggers }
    }

    /// Route an event to matching hats, ordered by specificity (most specific first).
    pub fn route(&self, event: &Event) -> Vec<String> {
        // System events are never routed to user hats
        if event.is_system() {
            return Vec::new();
        }

        let mut matches: Vec<(String, u32)> = Vec::new();

        for (hat_id, patterns) in &self.triggers {
            // If event is targeted, only consider the target hat
            if let Some(ref target) = event.target {
                if hat_id != target {
                    continue;
                }
            }

            // Find the best (most specific) matching trigger for this hat
            let best_specificity = patterns
                .iter()
                .filter(|(_, topic)| topic.matches(&event.topic))
                .map(|(_, topic)| topic.specificity())
                .max();

            if let Some(specificity) = best_specificity {
                matches.push((hat_id.clone(), specificity));
            }
        }

        // Sort by specificity descending (most specific first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));
        matches.into_iter().map(|(id, _)| id).collect()
    }

    /// Route with activation state — excludes hats that have exceeded max_activations.
    pub fn route_with_state(
        &self,
        event: &Event,
        activations: &HashMap<String, u32>,
    ) -> Vec<String> {
        self.route(event)
            .into_iter()
            .filter(|hat_id| {
                if let Some(hat) = self.hats.get(hat_id) {
                    if let Some(max) = hat.max_activations {
                        let count = activations.get(hat_id).copied().unwrap_or(0);
                        return count < max;
                    }
                }
                true
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hat(triggers: &[&str], publishes: &[&str]) -> HatConfig {
        HatConfig {
            name: "Test".to_string(),
            description: "test hat".to_string(),
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            publishes: publishes.iter().map(|s| s.to_string()).collect(),
            default_publishes: None,
            instructions: "test".to_string(),
            command: None,
            image: None,
            env: None,
            resources: None,
            max_activations: None,
            concurrency: None,
            gates: Vec::new(),
            secret_mounts: Vec::new(),
        }
    }

    fn event(topic: &str) -> Event {
        Event::new(topic, "test", None, "run-test", 1)
    }

    #[test]
    fn test_exact_match_routes_to_hat() {
        let mut hats = HashMap::new();
        hats.insert("worker".to_string(), hat(&["work.start"], &["work.done"]));

        let router = EventRouter::new(hats);
        let result = router.route(&event("work.start"));
        assert_eq!(result, vec!["worker"]);
    }

    #[test]
    fn test_no_match_returns_empty() {
        let mut hats = HashMap::new();
        hats.insert("worker".to_string(), hat(&["work.start"], &["work.done"]));

        let router = EventRouter::new(hats);
        let result = router.route(&event("other.topic"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_wildcard_match() {
        let mut hats = HashMap::new();
        hats.insert("worker".to_string(), hat(&["work.*"], &["work.done"]));

        let router = EventRouter::new(hats);
        assert_eq!(router.route(&event("work.start")), vec!["worker"]);
        assert_eq!(router.route(&event("work.done")), vec!["worker"]);
        assert!(router.route(&event("other.start")).is_empty());
    }

    #[test]
    fn test_multiple_hats_match_same_event() {
        let mut hats = HashMap::new();
        hats.insert("planner".to_string(), hat(&["work.start"], &["subtask.ready"]));
        hats.insert("logger".to_string(), hat(&[">"], &[]));

        let router = EventRouter::new(hats);
        let result = router.route(&event("work.start"));
        assert_eq!(result.len(), 2);
        // Exact match should come first (higher specificity)
        assert_eq!(result[0], "planner");
        assert_eq!(result[1], "logger");
    }

    #[test]
    fn test_targeted_event_only_routes_to_target() {
        let mut hats = HashMap::new();
        hats.insert("planner".to_string(), hat(&["work.start"], &["subtask.ready"]));
        hats.insert("builder".to_string(), hat(&["work.start"], &["work.done"]));

        let router = EventRouter::new(hats);
        let mut evt = event("work.start");
        evt.target = Some("builder".to_string());
        let result = router.route(&evt);
        assert_eq!(result, vec!["builder"]);
    }

    #[test]
    fn test_targeted_event_target_must_also_match_trigger() {
        let mut hats = HashMap::new();
        hats.insert("builder".to_string(), hat(&["subtask.ready"], &["work.done"]));

        let router = EventRouter::new(hats);
        let mut evt = event("work.start");
        evt.target = Some("builder".to_string());
        let result = router.route(&evt);
        assert!(result.is_empty());
    }

    #[test]
    fn test_max_activations_excludes_hat() {
        let mut hats = HashMap::new();
        let mut h = hat(&["work.start"], &["work.done"]);
        h.max_activations = Some(2);
        hats.insert("worker".to_string(), h);

        let router = EventRouter::new(hats);
        let mut activations = HashMap::new();
        activations.insert("worker".to_string(), 2u32);

        let result = router.route_with_state(&event("work.start"), &activations);
        assert!(result.is_empty());
    }

    #[test]
    fn test_max_activations_allows_under_limit() {
        let mut hats = HashMap::new();
        let mut h = hat(&["work.start"], &["work.done"]);
        h.max_activations = Some(5);
        hats.insert("worker".to_string(), h);

        let router = EventRouter::new(hats);
        let mut activations = HashMap::new();
        activations.insert("worker".to_string(), 3u32);

        let result = router.route_with_state(&event("work.start"), &activations);
        assert_eq!(result, vec!["worker"]);
    }

    #[test]
    fn test_multiple_triggers_on_one_hat() {
        let mut hats = HashMap::new();
        hats.insert("worker".to_string(), hat(&["work.start", "review.rejected"], &["work.done"]));

        let router = EventRouter::new(hats);
        assert_eq!(router.route(&event("work.start")), vec!["worker"]);
        assert_eq!(router.route(&event("review.rejected")), vec!["worker"]);
        assert!(router.route(&event("other")).is_empty());
    }

    #[test]
    fn test_system_events_not_routed() {
        let mut hats = HashMap::new();
        hats.insert("worker".to_string(), hat(&[">"], &["work.done"]));

        let router = EventRouter::new(hats);
        let sys = Event::system_completion("run-test", "LOOP_COMPLETE", 1);
        let result = router.route(&sys);
        assert!(result.is_empty());
    }
}
