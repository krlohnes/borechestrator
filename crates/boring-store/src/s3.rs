use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

use crate::traits::Store;

/// S3-compatible object store. Works with AWS S3, RustFS, MinIO, etc.
pub struct S3Store {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Store {
    /// Create from an existing S3 client.
    pub fn new(client: Client, bucket: &str, prefix: &str) -> Self {
        Self {
            client,
            bucket: bucket.to_string(),
            prefix: prefix.trim_end_matches('/').to_string(),
        }
    }

    /// Create configured for a local S3-compatible endpoint (RustFS, MinIO).
    pub async fn local(
        endpoint: &str,
        bucket: &str,
        prefix: &str,
        access_key: &str,
        secret_key: &str,
    ) -> anyhow::Result<Self> {
        let creds = aws_credential_types::Credentials::new(
            access_key,
            secret_key,
            None,
            None,
            "boring-store",
        );

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(creds)
            .endpoint_url(endpoint)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        let client = Client::from_conf(s3_config);

        // Ensure bucket exists
        match client.create_bucket().bucket(bucket).send().await {
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{}", e);
                // Ignore "bucket already exists" errors
                if !msg.contains("BucketAlreadyOwnedByYou") && !msg.contains("BucketAlreadyExists")
                {
                    // Some S3-compatible stores return different errors, just log and continue
                    tracing::debug!("create_bucket returned: {}", msg);
                }
            }
        }

        Ok(Self::new(client, bucket, prefix))
    }

    fn full_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.prefix, key)
        }
    }
}

#[async_trait]
impl Store for S3Store {
    async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let full_key = self.full_key(key);
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await
        {
            Ok(output) => {
                let bytes = output.body.collect().await?.into_bytes().to_vec();
                Ok(Some(bytes))
            }
            Err(e) => {
                let service_err = e.into_service_error();
                if service_err.is_no_such_key() {
                    Ok(None)
                } else {
                    Err(service_err.into())
                }
            }
        }
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> anyhow::Result<()> {
        let full_key = self.full_key(key);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .body(ByteStream::from(value))
            .send()
            .await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let full_prefix = self.full_key(prefix);
        let mut keys = Vec::new();
        let mut continuation_token = None;

        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&full_prefix);

            if let Some(token) = continuation_token {
                req = req.continuation_token(token);
            }

            let output = req.send().await?;

            if let Some(contents) = output.contents {
                for obj in contents {
                    if let Some(key) = obj.key {
                        // Strip the prefix to return relative keys
                        let relative = if self.prefix.is_empty() {
                            key
                        } else {
                            key.strip_prefix(&format!("{}/", self.prefix))
                                .unwrap_or(&key)
                                .to_string()
                        };
                        keys.push(relative);
                    }
                }
            }

            if output.is_truncated == Some(true) {
                continuation_token = output.next_continuation_token;
            } else {
                break;
            }
        }

        keys.sort();
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let full_key = self.full_key(key);
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_store() -> S3Store {
        let bucket = format!(
            "boring-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                % 1_000_000
        );

        S3Store::local(
            "http://localhost:9000",
            &bucket,
            "test",
            "rustfsadmin",
            "rustfsadmin",
        )
        .await
        .expect("RustFS must be running for S3 tests")
    }

    #[tokio::test]
    #[ignore] // requires RustFS/MinIO on localhost:9000
    async fn test_s3_put_and_get() {
        let store = setup_store().await;
        store
            .put("hello.txt", b"hello world".to_vec())
            .await
            .unwrap();

        let result = store.get("hello.txt").await.unwrap();
        assert_eq!(result, Some(b"hello world".to_vec()));
    }

    #[tokio::test]
    #[ignore]
    async fn test_s3_get_nonexistent() {
        let store = setup_store().await;
        let result = store.get("does-not-exist").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    #[ignore]
    async fn test_s3_put_overwrites() {
        let store = setup_store().await;
        store.put("key", b"first".to_vec()).await.unwrap();
        store.put("key", b"second".to_vec()).await.unwrap();

        let result = store.get("key").await.unwrap();
        assert_eq!(result, Some(b"second".to_vec()));
    }

    #[tokio::test]
    #[ignore]
    async fn test_s3_list_by_prefix() {
        let store = setup_store().await;
        store
            .put("run-abc/scratchpad/planner.md", b"a".to_vec())
            .await
            .unwrap();
        store
            .put("run-abc/scratchpad/builder.md", b"b".to_vec())
            .await
            .unwrap();
        store
            .put("run-abc/events/001.json", b"c".to_vec())
            .await
            .unwrap();

        let keys = store.list("run-abc/scratchpad/").await.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"run-abc/scratchpad/builder.md".to_string()));
        assert!(keys.contains(&"run-abc/scratchpad/planner.md".to_string()));
    }

    #[tokio::test]
    #[ignore]
    async fn test_s3_delete() {
        let store = setup_store().await;
        store.put("deleteme", b"gone".to_vec()).await.unwrap();
        store.delete("deleteme").await.unwrap();

        let result = store.get("deleteme").await.unwrap();
        assert_eq!(result, None);
    }
}
