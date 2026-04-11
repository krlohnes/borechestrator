use async_trait::async_trait;
use aws_sdk_secretsmanager::Client;

use crate::traits::SecretProvider;

/// Resolves secrets from AWS Secrets Manager.
///
/// Secret name maps directly to an AWS Secrets Manager secret name or ARN.
pub struct AwsSecretProvider {
    client: Client,
}

impl AwsSecretProvider {
    pub async fn new() -> anyhow::Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&config);
        Ok(Self { client })
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SecretProvider for AwsSecretProvider {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<String>> {
        match self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
        {
            Ok(output) => Ok(output.secret_string),
            Err(e) => {
                let service_err = e.into_service_error();
                if service_err.is_resource_not_found_exception() {
                    Ok(None)
                } else {
                    Err(service_err.into())
                }
            }
        }
    }

    async fn get_secret_bytes(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
        {
            Ok(output) => Ok(output.secret_binary.map(|b| b.into_inner())),
            Err(e) => {
                let service_err = e.into_service_error();
                if service_err.is_resource_not_found_exception() {
                    Ok(None)
                } else {
                    Err(service_err.into())
                }
            }
        }
    }
}
