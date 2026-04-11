use std::fmt;

/// A topic pattern with NATS-compatible wildcard matching.
///
/// - `*` matches exactly one segment
/// - `>` matches one or more trailing segments (must be last token)
/// - Everything else is an exact segment match
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Topic {
    inner: String,
}

impl Topic {
    pub fn new(pattern: &str) -> Self {
        Self {
            inner: pattern.to_string(),
        }
    }

    /// Check if this pattern matches the given subject string.
    pub fn matches(&self, subject: &str) -> bool {
        if self.inner.is_empty() {
            return false;
        }

        let pattern_parts: Vec<&str> = self.inner.split('.').collect();
        let subject_parts: Vec<&str> = subject.split('.').collect();

        let mut pi = 0;
        let mut si = 0;

        while pi < pattern_parts.len() && si < subject_parts.len() {
            match pattern_parts[pi] {
                ">" => {
                    // > matches one or more remaining segments, must be last
                    return si < subject_parts.len();
                }
                "*" => {
                    // * matches exactly one segment
                    pi += 1;
                    si += 1;
                }
                exact => {
                    if exact != subject_parts[si] {
                        return false;
                    }
                    pi += 1;
                    si += 1;
                }
            }
        }

        // Both must be exhausted (unless pattern ended with >)
        pi == pattern_parts.len() && si == subject_parts.len()
    }

    /// Whether this topic contains wildcard tokens.
    pub fn is_wildcard(&self) -> bool {
        self.inner.contains('*') || self.inner.contains('>')
    }

    /// Specificity score for routing priority. Higher = more specific.
    ///
    /// Exact segments score highest, `*` scores less, `>` scores least.
    pub fn specificity(&self) -> u32 {
        if self.inner.is_empty() {
            return 0;
        }
        let parts: Vec<&str> = self.inner.split('.').collect();
        let mut score = 0u32;
        for part in &parts {
            match *part {
                ">" => score += 1,
                "*" => score += 10,
                _ => score += 100,
            }
        }
        score
    }

    /// Get the inner pattern string.
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let topic = Topic::new("work.start");
        assert!(topic.matches("work.start"));
        assert!(!topic.matches("work.done"));
        assert!(!topic.matches("work.start.extra"));
        assert!(!topic.matches("other"));
    }

    #[test]
    fn test_single_wildcard_matches_one_segment() {
        let topic = Topic::new("work.*");
        assert!(topic.matches("work.start"));
        assert!(topic.matches("work.done"));
        assert!(!topic.matches("work.sub.deep"));
        assert!(!topic.matches("other.start"));
        assert!(!topic.matches("work"));
    }

    #[test]
    fn test_multi_wildcard_matches_trailing_segments() {
        let topic = Topic::new("work.>");
        assert!(topic.matches("work.start"));
        assert!(topic.matches("work.sub.deep"));
        assert!(topic.matches("work.a.b.c"));
        assert!(!topic.matches("other.start"));
        assert!(!topic.matches("work"));
    }

    #[test]
    fn test_global_wildcard_matches_everything() {
        let topic = Topic::new(">");
        assert!(topic.matches("work.start"));
        assert!(topic.matches("anything"));
        assert!(topic.matches("a.b.c.d"));
    }

    #[test]
    fn test_single_segment_exact() {
        let topic = Topic::new("start");
        assert!(topic.matches("start"));
        assert!(!topic.matches("start.extra"));
        assert!(!topic.matches("other"));
    }

    #[test]
    fn test_empty_pattern_matches_nothing() {
        let topic = Topic::new("");
        assert!(!topic.matches("work.start"));
        assert!(!topic.matches(""));
    }

    #[test]
    fn test_wildcard_in_middle() {
        let topic = Topic::new("work.*.done");
        assert!(topic.matches("work.build.done"));
        assert!(topic.matches("work.test.done"));
        assert!(!topic.matches("work.done"));
        assert!(!topic.matches("work.build.test.done"));
    }

    #[test]
    fn test_is_wildcard() {
        assert!(!Topic::new("work.start").is_wildcard());
        assert!(Topic::new("work.*").is_wildcard());
        assert!(Topic::new("work.>").is_wildcard());
        assert!(Topic::new(">").is_wildcard());
    }

    #[test]
    fn test_specificity_ordering() {
        let exact = Topic::new("work.start");
        let single = Topic::new("work.*");
        let multi = Topic::new("work.>");
        let global = Topic::new(">");

        assert!(exact.specificity() > single.specificity());
        assert!(single.specificity() > multi.specificity());
        assert!(multi.specificity() > global.specificity());
    }

    #[test]
    fn test_display() {
        let topic = Topic::new("work.start");
        assert_eq!(topic.to_string(), "work.start");
    }
}
