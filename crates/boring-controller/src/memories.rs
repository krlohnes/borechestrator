use boring_store::Store;
use serde::{Deserialize, Serialize};

/// A persistent memory entry learned during orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Type: pattern, decision, fix, context
    pub memory_type: String,
    /// The content/lesson learned
    pub content: String,
    /// Which hat created this memory
    pub source: String,
    /// When it was created (ISO 8601)
    pub timestamp: String,
}

/// Manages memories stored in the backing store.
pub struct MemoryStore {
    store_key: String,
}

impl MemoryStore {
    pub fn new(run_id: &str) -> Self {
        Self {
            store_key: format!("{}/memories.json", run_id),
        }
    }

    /// Load all memories from the store.
    pub async fn load(&self, store: &dyn Store) -> anyhow::Result<Vec<Memory>> {
        match store.get(&self.store_key).await? {
            Some(bytes) => {
                let memories: Vec<Memory> = serde_json::from_slice(&bytes)?;
                Ok(memories)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Append a new memory and save.
    pub async fn append(&self, store: &dyn Store, memory: Memory) -> anyhow::Result<()> {
        let mut memories = self.load(store).await?;
        memories.push(memory);
        let bytes = serde_json::to_vec_pretty(&memories)?;
        store.put(&self.store_key, bytes).await?;
        Ok(())
    }

    /// Format memories for injection into a prompt.
    pub fn format_for_prompt(memories: &[Memory], budget: usize) -> String {
        if memories.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        let mut total_len = 0;

        // Most recent memories first
        for memory in memories.iter().rev() {
            let entry = format!(
                "- [{}] {}: {}",
                memory.memory_type, memory.source, memory.content
            );
            if total_len + entry.len() > budget {
                break;
            }
            total_len += entry.len();
            parts.push(entry);
        }

        if parts.is_empty() {
            return String::new();
        }

        parts.reverse(); // Back to chronological
        format!("# Memories\n\n{}", parts.join("\n"))
    }
}

/// Parse a BORING_MEMORY line from agent stdout.
/// Format: BORING_MEMORY <type> <content>
pub fn parse_memory_line(line: &str, source: &str) -> Option<Memory> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("BORING_MEMORY ")?;
    let mut parts = rest.splitn(2, ' ');
    let memory_type = parts.next()?;
    let content = parts.next().unwrap_or("");

    if memory_type.is_empty() {
        return None;
    }

    Some(Memory {
        memory_type: memory_type.to_string(),
        content: content.to_string(),
        source: source.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_line_pattern() {
        let mem = parse_memory_line(
            "BORING_MEMORY pattern Always use snake_case for function names",
            "builder",
        )
        .unwrap();
        assert_eq!(mem.memory_type, "pattern");
        assert_eq!(mem.content, "Always use snake_case for function names");
        assert_eq!(mem.source, "builder");
    }

    #[test]
    fn test_parse_memory_line_decision() {
        let mem = parse_memory_line(
            "BORING_MEMORY decision Chose async-nats over lapin for NATS client",
            "planner",
        )
        .unwrap();
        assert_eq!(mem.memory_type, "decision");
        assert!(mem.content.contains("async-nats"));
    }

    #[test]
    fn test_parse_memory_line_no_content() {
        let mem = parse_memory_line("BORING_MEMORY fix", "worker").unwrap();
        assert_eq!(mem.memory_type, "fix");
        assert_eq!(mem.content, "");
    }

    #[test]
    fn test_parse_memory_line_not_a_memory() {
        assert!(parse_memory_line("just regular output", "worker").is_none());
        assert!(parse_memory_line("BORING_EMIT work.done", "worker").is_none());
    }

    #[test]
    fn test_format_for_prompt_empty() {
        assert_eq!(MemoryStore::format_for_prompt(&[], 1000), "");
    }

    #[test]
    fn test_format_for_prompt() {
        let memories = vec![
            Memory {
                memory_type: "pattern".to_string(),
                content: "Use snake_case".to_string(),
                source: "builder".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            Memory {
                memory_type: "decision".to_string(),
                content: "Chose NATS".to_string(),
                source: "planner".to_string(),
                timestamp: "2026-01-01T00:01:00Z".to_string(),
            },
        ];
        let result = MemoryStore::format_for_prompt(&memories, 1000);
        assert!(result.contains("# Memories"));
        assert!(result.contains("[pattern]"));
        assert!(result.contains("[decision]"));
        assert!(result.contains("Use snake_case"));
    }

    #[test]
    fn test_format_for_prompt_respects_budget() {
        let memories: Vec<Memory> = (0..100)
            .map(|i| Memory {
                memory_type: "pattern".to_string(),
                content: format!("Memory number {} with some padding text here", i),
                source: "worker".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            })
            .collect();
        let result = MemoryStore::format_for_prompt(&memories, 200);
        assert!(result.len() < 300); // budget + header
        assert!(result.contains("# Memories"));
    }

    #[test]
    fn test_memory_json_roundtrip() {
        let mem = Memory {
            memory_type: "fix".to_string(),
            content: "Added null check for user input".to_string(),
            source: "builder".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&mem).unwrap();
        let parsed: Memory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.memory_type, "fix");
        assert_eq!(parsed.content, "Added null check for user input");
    }
}
