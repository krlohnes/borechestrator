use async_trait::async_trait;

/// Abstraction over object storage for scratchpads, event archives, and artifacts.
#[async_trait]
pub trait Store: Send + Sync {
    /// Get an object by key. Returns None if not found.
    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>>;

    /// Put an object. Creates parent structure as needed.
    async fn put(&self, key: &str, value: Vec<u8>) -> anyhow::Result<()>;

    /// List keys under the given prefix.
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>>;

    /// Delete an object. No error if it doesn't exist.
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
}
