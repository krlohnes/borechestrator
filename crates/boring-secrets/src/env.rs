use crate::traits::SecretProvider;
use async_trait::async_trait;

/// Resolves secrets from environment variables.
///
/// Secret name `api-key` maps to env var `BORING_SECRET_API_KEY`
/// (uppercased, hyphens replaced with underscores, prefixed).
pub struct EnvSecretProvider {
    prefix: String,
}

impl EnvSecretProvider {
    pub fn new() -> Self {
        Self {
            prefix: "BORING_SECRET_".to_string(),
        }
    }

    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    fn env_var_name(&self, secret_name: &str) -> String {
        format!(
            "{}{}",
            self.prefix,
            secret_name.to_uppercase().replace('-', "_")
        )
    }
}

impl Default for EnvSecretProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretProvider for EnvSecretProvider {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
        let var_name = self.env_var_name(name);
        match std::env::var(&var_name) {
            Ok(val) => Ok(Some(val)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_name_mapping() {
        let provider = EnvSecretProvider::new();
        assert_eq!(provider.env_var_name("api-key"), "BORING_SECRET_API_KEY");
        assert_eq!(
            provider.env_var_name("my-secret"),
            "BORING_SECRET_MY_SECRET"
        );
        assert_eq!(
            provider.env_var_name("ALREADY_UPPER"),
            "BORING_SECRET_ALREADY_UPPER"
        );
    }

    #[test]
    fn test_custom_prefix() {
        let provider = EnvSecretProvider::with_prefix("MYAPP_");
        assert_eq!(provider.env_var_name("api-key"), "MYAPP_API_KEY");
    }

    #[tokio::test]
    async fn test_get_secret_from_env() {
        std::env::set_var("BORING_SECRET_TEST_KEY", "secret_value_123");
        let provider = EnvSecretProvider::new();

        let result = provider.get_secret("test-key").await.unwrap();
        assert_eq!(result, Some("secret_value_123".to_string()));

        std::env::remove_var("BORING_SECRET_TEST_KEY");
    }

    #[tokio::test]
    async fn test_get_secret_not_found() {
        let provider = EnvSecretProvider::new();
        let result = provider.get_secret("nonexistent-key-12345").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_has_secret() {
        std::env::set_var("BORING_SECRET_HAS_CHECK", "yes");
        let provider = EnvSecretProvider::new();

        assert!(provider.has_secret("has-check").await.unwrap());
        assert!(!provider.has_secret("nope-not-here").await.unwrap());

        std::env::remove_var("BORING_SECRET_HAS_CHECK");
    }
}
