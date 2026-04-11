use async_trait::async_trait;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};

use crate::traits::SecretProvider;

/// Resolves secrets from Kubernetes Secrets in the specified namespace.
///
/// Secret name maps to a K8s Secret resource name. The value is read
/// from the `value` key in the Secret's data field. For secrets with
/// multiple keys, use `name/key` format (e.g., `my-secret/api-key`).
pub struct K8sSecretProvider {
    client: Client,
    namespace: String,
}

impl K8sSecretProvider {
    pub async fn new(namespace: &str) -> anyhow::Result<Self> {
        let client = Client::try_default().await?;
        Ok(Self {
            client,
            namespace: namespace.to_string(),
        })
    }

    pub fn with_client(client: Client, namespace: &str) -> Self {
        Self {
            client,
            namespace: namespace.to_string(),
        }
    }

    fn parse_name(name: &str) -> (&str, &str) {
        match name.split_once('/') {
            Some((secret_name, key)) => (secret_name, key),
            None => (name, "value"),
        }
    }
}

#[async_trait]
impl SecretProvider for K8sSecretProvider {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
        let (secret_name, key) = Self::parse_name(name);
        let secrets: Api<Secret> = Api::namespaced(self.client.clone(), &self.namespace);

        match secrets.get_opt(secret_name).await? {
            Some(secret) => {
                if let Some(data) = secret.data {
                    if let Some(bytes) = data.get(key) {
                        return Ok(Some(String::from_utf8_lossy(&bytes.0).to_string()));
                    }
                }
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn get_secret_bytes(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let (secret_name, key) = Self::parse_name(name);
        let secrets: Api<Secret> = Api::namespaced(self.client.clone(), &self.namespace);

        match secrets.get_opt(secret_name).await? {
            Some(secret) => {
                if let Some(data) = secret.data {
                    if let Some(bytes) = data.get(key) {
                        return Ok(Some(bytes.0.clone()));
                    }
                }
                Ok(None)
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_with_key() {
        let (name, key) = K8sSecretProvider::parse_name("my-secret/api-key");
        assert_eq!(name, "my-secret");
        assert_eq!(key, "api-key");
    }

    #[test]
    fn test_parse_name_without_key() {
        let (name, key) = K8sSecretProvider::parse_name("my-secret");
        assert_eq!(name, "my-secret");
        assert_eq!(key, "value");
    }
}
