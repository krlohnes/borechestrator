use std::path::{Path, PathBuf};
use async_trait::async_trait;

use crate::traits::Store;

/// Local filesystem store. Maps keys to `{base_dir}/{key}` paths.
pub struct LocalStore {
    base_dir: PathBuf,
}

impl LocalStore {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn key_path(&self, key: &str) -> PathBuf {
        self.base_dir.join(key)
    }
}

#[async_trait]
impl Store for LocalStore {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let path = self.key_path(key);
        match tokio::fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> anyhow::Result<()> {
        let path = self.key_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, value).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let dir = self.key_path(prefix);
        let mut keys = Vec::new();

        if !dir.exists() {
            return Ok(keys);
        }

        let mut stack = vec![dir];
        while let Some(current) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&current).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(relative) = path.strip_prefix(&self.base_dir) {
                    keys.push(relative.to_string_lossy().to_string());
                }
            }
        }

        keys.sort();
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let path = self.key_path(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (LocalStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = LocalStore::new(dir.path());
        (store, dir)
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let (store, _dir) = setup();
        store.put("run-abc/scratchpad/planner.md", b"hello world".to_vec()).await.unwrap();

        let result = store.get("run-abc/scratchpad/planner.md").await.unwrap();
        assert_eq!(result, Some(b"hello world".to_vec()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let (store, _dir) = setup();
        let result = store.get("does/not/exist").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_put_overwrites() {
        let (store, _dir) = setup();
        store.put("key", b"first".to_vec()).await.unwrap();
        store.put("key", b"second".to_vec()).await.unwrap();

        let result = store.get("key").await.unwrap();
        assert_eq!(result, Some(b"second".to_vec()));
    }

    #[tokio::test]
    async fn test_put_creates_parent_dirs() {
        let (store, _dir) = setup();
        store.put("a/b/c/deep.txt", b"deep".to_vec()).await.unwrap();

        let result = store.get("a/b/c/deep.txt").await.unwrap();
        assert_eq!(result, Some(b"deep".to_vec()));
    }

    #[tokio::test]
    async fn test_list_by_prefix() {
        let (store, _dir) = setup();
        store.put("run-abc/scratchpad/planner.md", b"a".to_vec()).await.unwrap();
        store.put("run-abc/scratchpad/builder.md", b"b".to_vec()).await.unwrap();
        store.put("run-abc/events/001.json", b"c".to_vec()).await.unwrap();
        store.put("run-other/scratchpad/x.md", b"d".to_vec()).await.unwrap();

        let mut keys = store.list("run-abc/scratchpad/").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec![
            "run-abc/scratchpad/builder.md",
            "run-abc/scratchpad/planner.md",
        ]);
    }

    #[tokio::test]
    async fn test_list_empty_prefix() {
        let (store, _dir) = setup();
        let keys = store.list("nonexistent/").await.unwrap();
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn test_delete() {
        let (store, _dir) = setup();
        store.put("key", b"value".to_vec()).await.unwrap();
        store.delete("key").await.unwrap();

        let result = store.get("key").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_is_ok() {
        let (store, _dir) = setup();
        let result = store.delete("does/not/exist").await;
        assert!(result.is_ok());
    }
}
