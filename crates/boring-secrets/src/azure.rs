use async_trait::async_trait;
use azure_identity::DefaultAzureCredential;
use azure_security_keyvault::SecretClient;

use crate::traits::SecretProvider;

/// Resolves secrets from Azure Key Vault.
///
/// Secret name maps to an Azure Key Vault secret name.
/// Uses DefaultAzureCredential for auth (env vars, managed identity, CLI, etc.)
pub struct AzureKeyVaultProvider {
    client: SecretClient,
}

impl AzureKeyVaultProvider {
    pub fn new(vault_url: &str) -> anyhow::Result<Self> {
        let credential = DefaultAzureCredential::new()?;
        let client = SecretClient::new(vault_url, credential)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SecretProvider for AzureKeyVaultProvider {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
        match self.client.get(name).await {
            Ok(response) => Ok(Some(response.value)),
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("SecretNotFound") || msg.contains("404") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
}
