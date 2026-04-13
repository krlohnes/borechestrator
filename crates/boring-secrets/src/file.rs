use crate::traits::SecretProvider;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Resolves secrets from files on disk.
///
/// Useful for mounting credential files (e.g., `~/.claude/credentials.json`,
/// SSH keys, service account tokens). Secret name maps to a file path
/// under the configured base directory.
pub struct FileSecretProvider {
    base_dir: PathBuf,
}

impl FileSecretProvider {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn secret_path(&self, name: &str) -> PathBuf {
        self.base_dir.join(name)
    }
}

#[async_trait]
impl SecretProvider for FileSecretProvider {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
        let path = self.secret_path(name);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => Ok(Some(content.trim_end().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_secret_bytes(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let path = self.secret_path(name);
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_get_secret_from_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("api-key"), "sk-12345\n").unwrap();

        let provider = FileSecretProvider::new(dir.path());
        let result = provider.get_secret("api-key").await.unwrap();
        assert_eq!(result, Some("sk-12345".to_string()));
    }

    #[tokio::test]
    async fn test_get_secret_trims_trailing_newline() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("token"), "abc123\n\n").unwrap();

        let provider = FileSecretProvider::new(dir.path());
        let result = provider.get_secret("token").await.unwrap();
        assert_eq!(result, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn test_get_secret_not_found() {
        let dir = TempDir::new().unwrap();
        let provider = FileSecretProvider::new(dir.path());
        let result = provider.get_secret("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_get_secret_bytes() {
        let dir = TempDir::new().unwrap();
        let binary_data = vec![0x00, 0x01, 0xFF, 0xFE];
        std::fs::write(dir.path().join("binary-key"), &binary_data).unwrap();

        let provider = FileSecretProvider::new(dir.path());
        let result = provider.get_secret_bytes("binary-key").await.unwrap();
        assert_eq!(result, Some(binary_data));
    }

    #[tokio::test]
    async fn test_subdirectory_secret() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("claude")).unwrap();
        std::fs::write(
            dir.path().join("claude/credentials.json"),
            r#"{"key":"val"}"#,
        )
        .unwrap();

        let provider = FileSecretProvider::new(dir.path());
        let result = provider
            .get_secret("claude/credentials.json")
            .await
            .unwrap();
        assert_eq!(result, Some(r#"{"key":"val"}"#.to_string()));
    }

    #[tokio::test]
    async fn test_has_secret() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("exists"), "yes").unwrap();

        let provider = FileSecretProvider::new(dir.path());
        assert!(provider.has_secret("exists").await.unwrap());
        assert!(!provider.has_secret("nope").await.unwrap());
    }
}
