use async_trait::async_trait;
use crate::traits::SecretProvider;

/// Tries multiple secret providers in order, returning the first match.
///
/// Typical chain: env vars → files → K8s secrets → AWS Secrets Manager.
/// Stops at the first provider that returns `Some`.
pub struct ChainSecretProvider {
    providers: Vec<Box<dyn SecretProvider>>,
}

impl ChainSecretProvider {
    pub fn new(providers: Vec<Box<dyn SecretProvider>>) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl SecretProvider for ChainSecretProvider {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
        for provider in &self.providers {
            if let Some(value) = provider.get_secret(name).await? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    async fn get_secret_bytes(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        for provider in &self.providers {
            if let Some(value) = provider.get_secret_bytes(name).await? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::EnvSecretProvider;
    use crate::file::FileSecretProvider;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_chain_returns_first_match() {
        // Set up env with a value
        std::env::set_var("BORING_SECRET_CHAIN_TEST", "from_env");

        // Set up file with a different value for the same secret
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("chain-test"), "from_file").unwrap();

        let chain = ChainSecretProvider::new(vec![
            Box::new(EnvSecretProvider::new()),
            Box::new(FileSecretProvider::new(dir.path())),
        ]);

        // Env provider is first, so it wins
        let result = chain.get_secret("chain-test").await.unwrap();
        assert_eq!(result, Some("from_env".to_string()));

        std::env::remove_var("BORING_SECRET_CHAIN_TEST");
    }

    #[tokio::test]
    async fn test_chain_falls_through() {
        // No env var set, but file exists
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("fallthrough-key"), "from_file").unwrap();

        let chain = ChainSecretProvider::new(vec![
            Box::new(EnvSecretProvider::new()),
            Box::new(FileSecretProvider::new(dir.path())),
        ]);

        let result = chain.get_secret("fallthrough-key").await.unwrap();
        assert_eq!(result, Some("from_file".to_string()));
    }

    #[tokio::test]
    async fn test_chain_none_if_no_match() {
        let dir = TempDir::new().unwrap();

        let chain = ChainSecretProvider::new(vec![
            Box::new(EnvSecretProvider::new()),
            Box::new(FileSecretProvider::new(dir.path())),
        ]);

        let result = chain.get_secret("does-not-exist-anywhere").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_empty_chain_returns_none() {
        let chain = ChainSecretProvider::new(vec![]);
        let result = chain.get_secret("anything").await.unwrap();
        assert_eq!(result, None);
    }
}
