use boring_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serializable snapshot of the reconciler's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub iterations: u32,
    pub activations: HashMap<String, u32>,
    pub consecutive_failures: u32,
    pub global_sequence: u64,
    pub seen_events: Vec<String>,
    pub config_hash: String,
}

impl Checkpoint {
    pub fn store_key(run_id: &str) -> String {
        format!("{}/checkpoint.json", run_id)
    }

    /// Save checkpoint to the store.
    pub async fn save(&self, store: &dyn Store) -> anyhow::Result<()> {
        let key = Self::store_key(&self.run_id);
        let bytes = serde_json::to_vec_pretty(self)?;
        store.put(&key, bytes).await?;
        Ok(())
    }

    /// Load checkpoint from the store. Returns None if not found.
    pub async fn load(store: &dyn Store, run_id: &str) -> anyhow::Result<Option<Self>> {
        let key = Self::store_key(run_id);
        match store.get(&key).await? {
            Some(bytes) => {
                let checkpoint: Checkpoint = serde_json::from_slice(&bytes)?;
                Ok(Some(checkpoint))
            }
            None => Ok(None),
        }
    }

    /// Find the most recent checkpoint across all runs.
    pub async fn find_latest(store: &dyn Store) -> anyhow::Result<Option<Self>> {
        // List all checkpoint files
        let keys = store.list("").await?;
        let checkpoint_keys: Vec<&String> = keys
            .iter()
            .filter(|k| k.ends_with("/checkpoint.json"))
            .collect();

        let mut latest: Option<Checkpoint> = None;

        for key in checkpoint_keys {
            if let Some(bytes) = store.get(key).await? {
                if let Ok(cp) = serde_json::from_slice::<Checkpoint>(&bytes) {
                    latest = Some(cp);
                }
            }
        }

        Ok(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boring_store::LocalStore;
    use tempfile::TempDir;

    fn test_checkpoint() -> Checkpoint {
        Checkpoint {
            run_id: "run-abc".to_string(),
            iterations: 5,
            activations: [("planner".to_string(), 3u32), ("builder".to_string(), 2u32)].into(),
            consecutive_failures: 0,
            global_sequence: 10,
            seen_events: vec!["work.start".to_string(), "subtask.ready".to_string()],
            config_hash: "abc123".to_string(),
        }
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let store = LocalStore::new(dir.path());

        let cp = test_checkpoint();
        cp.save(&store).await.unwrap();

        let loaded = Checkpoint::load(&store, "run-abc").await.unwrap().unwrap();
        assert_eq!(loaded.run_id, "run-abc");
        assert_eq!(loaded.iterations, 5);
        assert_eq!(loaded.activations.get("planner"), Some(&3));
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let dir = TempDir::new().unwrap();
        let store = LocalStore::new(dir.path());

        let result = Checkpoint::load(&store, "no-such-run").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_json_roundtrip() {
        let cp = test_checkpoint();
        let json = serde_json::to_string(&cp).unwrap();
        let parsed: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.global_sequence, 10);
        assert_eq!(parsed.seen_events.len(), 2);
    }
}
