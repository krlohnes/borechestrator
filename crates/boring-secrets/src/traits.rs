use async_trait::async_trait;

/// Abstraction over secret storage backends.
///
/// Implementations resolve a secret name to its value. The name format
/// is opaque — it might be a K8s secret name, an AWS ARN, an env var
/// suffix, or a file path depending on the backend.
#[async_trait]
pub trait SecretProvider: Send + Sync {
    /// Get a secret value as a string. Returns None if not found.
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>>;

    /// Get a secret value as raw bytes. Returns None if not found.
    /// Default implementation converts from get_secret.
    async fn get_secret_bytes(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.get_secret(name).await?.map(|s| s.into_bytes()))
    }

    /// Check if a secret exists without retrieving its value.
    async fn has_secret(&self, name: &str) -> anyhow::Result<bool> {
        Ok(self.get_secret(name).await?.is_some())
    }
}
